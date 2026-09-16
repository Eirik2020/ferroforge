#![no_std]

//! FerroForge is a host-side, RTIC-like application model.
//!
//! It re-exports the [`app!`], [`dependency_registry!`], and [`task`]
//! procedural macros together with the small runtime types used by their
//! generated code.

#[cfg(not(target_arch = "arm"))]
extern crate std;

use core::{
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

pub use ferroforge_macros::{
    app, composition, dependency_registry, firmware, init, reusable, task,
};
pub use fugit::{ExtU64, MillisDurationU64};

/// Compile-only interfaces used by standalone reusable source checks.
pub mod mock {
    pub mod systick {
        /// The initial supported monotonic profile: core SysTick at 1 kHz with
        /// `u32` time values.
        #[derive(Debug, Clone, Copy, Default)]
        pub struct Mono;

        impl Mono {
            /// Type-check a delay without providing a scheduler or clock.
            pub async fn delay(_duration: fugit::Duration<u32, 1, 1_000>) {
                panic!("the standalone SysTick mock is compile-only")
            }
        }
    }
}

/// Cargo metadata owned by an application's central dependency registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyDefinition<Id> {
    pub id: Id,
    pub package: &'static str,
    pub version: &'static str,
    pub default_features: bool,
    pub features: &'static [&'static str],
}

/// A dependency and the additional Cargo features requested by one task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyRequirement<Id> {
    pub dependency: Id,
    pub features: &'static [&'static str],
}

/// Renderer-facing Cargo dependency metadata with a stable textual ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyCatalogEntry {
    pub id: &'static str,
    pub package: &'static str,
    pub version: &'static str,
    pub default_features: bool,
    pub features: &'static [&'static str],
}

/// Renderer-facing dependency requirement declared by a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDependencyDefinition {
    pub id: &'static str,
    pub features: &'static [&'static str],
}

/// The source-level portion of a reusable task definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskImplementationDefinition {
    pub source: &'static str,
    pub shared_resources: &'static [&'static str],
    pub local_resources: &'static [&'static str],
    pub config_keys: &'static [&'static str],
    pub spawn_aliases: &'static [&'static str],
    pub dependencies: &'static [TaskDependencyDefinition],
}

/// One configured task constant from the application declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskConfigurationDefinition {
    pub name: &'static str,
    pub rust_type: &'static str,
    pub value: &'static str,
}

/// A task-local spawn alias resolved by the application declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSpawnBindingDefinition {
    pub alias: &'static str,
    pub target: &'static str,
}

/// One task selected and configured by a host-side application composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposedTaskDefinition {
    pub name: &'static str,
    pub priority: u8,
    pub interrupt: Option<&'static str>,
    pub configuration: &'static [TaskConfigurationDefinition],
    pub spawn_bindings: &'static [TaskSpawnBindingDefinition],
}

/// Host-side scheduling and configuration applied to embedded task sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionDefinition {
    pub dispatchers: &'static [&'static str],
    pub tasks: &'static [ComposedTaskDefinition],
}

/// Complete renderer-facing definition of one configured task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDefinition {
    pub name: &'static str,
    pub priority: u8,
    pub interrupt: Option<&'static str>,
    pub implementation: TaskImplementationDefinition,
    pub configuration: &'static [TaskConfigurationDefinition],
    pub spawn_bindings: &'static [TaskSpawnBindingDefinition],
}

/// Linker memory-region data resolved from the selected MCU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegionDefinition {
    pub origin: u32,
    pub size_bytes: u32,
}

/// Renderer-facing target and backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetDefinition {
    pub mcu: &'static str,
    pub hal: &'static str,
    pub rust_target: &'static str,
    pub hal_feature: &'static str,
    pub rtic_monotonics_feature: &'static str,
    pub flash: MemoryRegionDefinition,
    pub ram: MemoryRegionDefinition,
}

/// Hardware source selected for a monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonotonicSourceDefinition {
    SysTick,
    Timer(&'static str),
}

/// Renderer-facing monotonic declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonotonicDefinition {
    pub name: &'static str,
    pub source: MonotonicSourceDefinition,
    pub tick_hz: u32,
}

/// Complete application declaration consumed by a firmware renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationDefinition {
    pub manifest_dir: &'static str,
    pub source_file: &'static str,
    pub module: &'static str,
    pub target: Option<TargetDefinition>,
    pub dispatchers: &'static [&'static str],
    pub monotonic: Option<MonotonicDefinition>,
    pub shared_source: &'static str,
    pub local_source: &'static str,
    pub init_source: &'static str,
    pub tasks: &'static [TaskDefinition],
    pub dependencies: &'static [DependencyCatalogEntry],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    QueueFull,
}

/// Associates a task's resource marker with the type declared by the app.
pub trait SharedResourceSpec {
    type Value;
}

/// Associates a task's local-resource marker with the type declared by the app.
pub trait LocalResourceSpec {
    type Value;
}

/// Zero-sized, compile-only stand-in for an RTIC shared-resource proxy.
pub struct MockShared<Resource> {
    marker: PhantomData<fn() -> Resource>,
}

/// Compile-only stand-in for a standalone task's typed shared-resource proxy.
pub struct MockSharedRef<'resource, Resource: ?Sized> {
    marker: PhantomData<&'resource mut Resource>,
}

impl<Resource: ?Sized> MockSharedRef<'_, Resource> {
    /// Type-checks an RTIC-like lock closure without providing runtime storage.
    #[track_caller]
    pub fn lock<Output>(&mut self, _f: impl FnOnce(&mut Resource) -> Output) -> Output {
        panic!("mock shared resources are not executable")
    }
}

impl<Resource> MockShared<Resource> {
    pub const fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<Resource> Default for MockShared<Resource> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Resource> Clone for MockShared<Resource> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Resource> Copy for MockShared<Resource> {}

impl<Resource> fmt::Debug for MockShared<Resource> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MockShared")
    }
}

impl<Resource> MockShared<Resource>
where
    Resource: SharedResourceSpec,
{
    /// Type-checks an RTIC-like lock closure without providing runtime storage.
    #[track_caller]
    pub fn lock<Output>(&mut self, _f: impl FnOnce(&mut Resource::Value) -> Output) -> Output {
        panic!("mock shared resources are not executable")
    }
}

/// Zero-sized, compile-only stand-in for an RTIC local-resource reference.
pub struct MockLocal<Resource> {
    marker: PhantomData<fn() -> Resource>,
}

impl<Resource> MockLocal<Resource> {
    pub const fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<Resource> Default for MockLocal<Resource> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Resource> Clone for MockLocal<Resource> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Resource> Copy for MockLocal<Resource> {}

impl<Resource> fmt::Debug for MockLocal<Resource> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MockLocal")
    }
}

impl<Resource> Deref for MockLocal<Resource>
where
    Resource: LocalResourceSpec,
{
    type Target = Resource::Value;

    #[track_caller]
    fn deref(&self) -> &Self::Target {
        panic!("mock local resources are not executable")
    }
}

impl<Resource> DerefMut for MockLocal<Resource>
where
    Resource: LocalResourceSpec,
{
    #[track_caller]
    fn deref_mut(&mut self) -> &mut Self::Target {
        panic!("mock local resources are not executable")
    }
}

/// Host-side stand-in for a named firmware monotonic.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockMonotonic<const TICK_HZ: u32>;

impl<const TICK_HZ: u32> MockMonotonic<TICK_HZ> {
    pub const TICK_HZ: u32 = TICK_HZ;

    /// Type-checks monotonic initialization in a firmware declaration.
    pub fn start(_clock_hz: u32) {}

    /// Delays on the host while preserving the firmware task's monotonic API.
    pub async fn delay(duration: MillisDurationU64) {
        delay(duration).await;
    }
}

/// Mock delay used by async task examples.
///
/// This blocks the current thread and is not a real RTIC monotonic timer.
pub async fn delay(duration: MillisDurationU64) {
    #[cfg(not(target_arch = "arm"))]
    std::thread::sleep(core::time::Duration::from_millis(duration.ticks()));

    #[cfg(target_arch = "arm")]
    {
        let _ = duration;
        panic!("mock delays are not executable in firmware checks");
    }
}
