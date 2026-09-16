//! Structural validation for standalone task composition.
//!
//! This is an owned host-side model, not a settled author-facing macro syntax.
//! It validates facts available from task declarations and leaves concrete Rust
//! type, lifetime, ownership, and RTIC checks to the generated application.

use std::collections::{BTreeMap, BTreeSet};

use ferroforge_contracts::identifier_key;
pub use ferroforge_contracts::TaskKind;
use quote::ToTokens;
use syn::{Expr, Ident, Type};

use crate::{
    RenderError,
    source::{DefinitionId, TaskInstance, TaskPackage, TaskSources, select_across},
};

pub const INITIAL_SYSTICK_TICK_HZ: u32 = 1_000;
pub const INITIAL_SYSTICK_COUNTER_BITS: u8 = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonotonicSource {
    SysTick,
    Timer(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonotonicProfile {
    pub source: MonotonicSource,
    pub tick_hz: u32,
    pub counter_bits: u8,
}

impl MonotonicProfile {
    pub const fn initial_systick() -> Self {
        Self {
            source: MonotonicSource::SysTick,
            tick_hz: INITIAL_SYSTICK_TICK_HZ,
            counter_bits: INITIAL_SYSTICK_COUNTER_BITS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceBinding {
    pub requirement: String,
    pub resource: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationBinding {
    pub name: String,
    pub rust_type: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnBinding {
    pub alias: String,
    pub target: String,
}

#[derive(Clone, Debug)]
pub struct TaskSelection {
    pub instance: String,
    pub definition: DefinitionId,
    pub priority: u8,
    /// The interrupt this instance binds, for hardware tasks only. Per G3 a
    /// hardware task requires one and a software task must not have one.
    pub interrupt: Option<String>,
    pub local: Vec<ResourceBinding>,
    pub shared: Vec<ResourceBinding>,
    pub configuration: Vec<ConfigurationBinding>,
    pub spawn: Vec<SpawnBinding>,
}

#[derive(Clone, Debug)]
pub struct StandaloneComposition {
    pub tasks: Vec<TaskSelection>,
    pub monotonic: Option<MonotonicProfile>,
}

#[derive(Clone, Debug)]
pub struct ValidatedTask<'sources> {
    pub source: TaskInstance<'sources>,
    pub priority: u8,
    pub interrupt: Option<String>,
    pub local: Vec<ResourceBinding>,
    pub shared: Vec<ResourceBinding>,
    pub configuration: Vec<ConfigurationBinding>,
    pub spawn: Vec<SpawnBinding>,
}

impl ValidatedTask<'_> {
    pub fn kind(&self) -> TaskKind {
        self.source.definition.contract.kind
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedComposition<'sources> {
    pub tasks: Vec<ValidatedTask<'sources>>,
    pub monotonic: Option<MonotonicProfile>,
}

/// Validate a composition whose definitions all come from one package.
pub fn validate_composition<'sources>(
    sources: &'sources TaskSources,
    composition: &StandaloneComposition,
) -> Result<ValidatedComposition<'sources>, RenderError> {
    let selections = selections(composition);
    let instances = sources.select(&selections)?;
    validate_selected(instances, composition)
}

/// Validate a composition drawing definitions from several packages, so a
/// firmware can select portable software tasks and HAL-specific hardware tasks
/// in one graph.
pub fn validate_composition_across<'sources>(
    packages: &[&'sources TaskPackage],
    composition: &StandaloneComposition,
) -> Result<ValidatedComposition<'sources>, RenderError> {
    let selections = selections(composition);
    let instances = select_across(packages, &selections)?;
    validate_selected(instances, composition)
}

fn selections(composition: &StandaloneComposition) -> Vec<(&str, DefinitionId)> {
    composition
        .tasks
        .iter()
        .map(|task| (task.instance.as_str(), task.definition.clone()))
        .collect()
}

fn validate_selected<'sources>(
    instances: Vec<TaskInstance<'sources>>,
    composition: &StandaloneComposition,
) -> Result<ValidatedComposition<'sources>, RenderError> {
    validate_monotonic(&instances, composition.monotonic.as_ref())?;

    let mut by_name = BTreeMap::new();
    for instance in &instances {
        let name = identifier_key(&identifier(&instance.name, "task instance")?);
        by_name.insert(name, instance);
    }
    let mut resource_categories = BTreeMap::new();
    let mut interrupt_owners: BTreeMap<String, String> = BTreeMap::new();
    let mut tasks = Vec::with_capacity(instances.len());

    for (selection, instance) in composition.tasks.iter().zip(&instances) {
        validate_resources(
            instance.name.as_str(),
            instance,
            selection,
            &mut resource_categories,
        )?;
        validate_configuration(instance.name.as_str(), instance, selection)?;
        validate_spawns(instance.name.as_str(), instance, selection, &by_name)?;
        validate_interrupt(
            instance.name.as_str(),
            instance,
            selection,
            &mut interrupt_owners,
        )?;
        tasks.push(ValidatedTask {
            source: instance.clone(),
            priority: selection.priority,
            interrupt: selection.interrupt.clone(),
            local: selection.local.clone(),
            shared: selection.shared.clone(),
            configuration: selection.configuration.clone(),
            spawn: selection.spawn.clone(),
        });
    }

    Ok(ValidatedComposition {
        tasks,
        monotonic: composition.monotonic.clone(),
    })
}

/// G3: hardware tasks require an interrupt binding, software tasks do not.
/// Two instances cannot own the same interrupt.
fn validate_interrupt(
    instance_name: &str,
    instance: &TaskInstance<'_>,
    selection: &TaskSelection,
    owners: &mut BTreeMap<String, String>,
) -> Result<(), RenderError> {
    match (instance.definition.contract.kind, &selection.interrupt) {
        (TaskKind::Hardware, None) => Err(invalid(format!(
            "hardware task instance `{instance_name}` requires an interrupt binding"
        ))),
        (TaskKind::Software, Some(interrupt)) => Err(invalid(format!(
            "software task instance `{instance_name}` cannot bind interrupt `{interrupt}`"
        ))),
        (TaskKind::Software, None) => Ok(()),
        (TaskKind::Hardware, Some(interrupt)) => {
            if let Some(owner) = owners.get(interrupt) {
                return Err(invalid(format!(
                    "interrupt `{interrupt}` is bound by both `{owner}` and `{instance_name}`"
                )));
            }
            owners.insert(interrupt.clone(), instance_name.to_owned());
            Ok(())
        }
    }
}

fn validate_monotonic(
    instances: &[TaskInstance<'_>],
    profile: Option<&MonotonicProfile>,
) -> Result<(), RenderError> {
    let requiring = instances
        .iter()
        .find(|instance| instance.definition.contract.arguments.monotonic.is_some());
    if let Some(instance) = requiring
        && profile.is_none()
    {
        return Err(invalid(format!(
            "task instance `{}` requires a monotonic profile",
            instance.name
        )));
    }
    if let Some(profile) = profile
        && profile != &MonotonicProfile::initial_systick()
    {
        return Err(invalid(format!(
            "unsupported monotonic profile: initial support requires SysTick at {} Hz with u{} time values",
            INITIAL_SYSTICK_TICK_HZ, INITIAL_SYSTICK_COUNTER_BITS
        )));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceCategory {
    Local,
    Shared,
}

impl ResourceCategory {
    const fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Shared => "shared",
        }
    }
}

fn validate_resources(
    instance_name: &str,
    instance: &TaskInstance<'_>,
    selection: &TaskSelection,
    resource_categories: &mut BTreeMap<String, ResourceCategory>,
) -> Result<(), RenderError> {
    validate_resource_category(
        instance_name,
        &instance.definition.contract.arguments.local,
        &instance.definition.contract.arguments.shared,
        &selection.local,
        ResourceCategory::Local,
        resource_categories,
    )?;
    validate_resource_category(
        instance_name,
        &instance.definition.contract.arguments.shared,
        &instance.definition.contract.arguments.local,
        &selection.shared,
        ResourceCategory::Shared,
        resource_categories,
    )
}

fn validate_resource_category(
    instance: &str,
    declared: &[ferroforge_contracts::Resource],
    other_category: &[ferroforge_contracts::Resource],
    bindings: &[ResourceBinding],
    category: ResourceCategory,
    resource_categories: &mut BTreeMap<String, ResourceCategory>,
) -> Result<(), RenderError> {
    let declared = declared
        .iter()
        .map(|resource| identifier_key(&resource.name))
        .collect::<BTreeSet<_>>();
    let other_category = other_category
        .iter()
        .map(|resource| identifier_key(&resource.name))
        .collect::<BTreeSet<_>>();
    let mut bound_requirements = BTreeSet::new();
    let mut bound_resources = BTreeSet::new();

    for binding in bindings {
        let requirement = identifier(&binding.requirement, "resource requirement")?;
        let requirement = identifier_key(&requirement);
        let resource = identifier(&binding.resource, "system resource")?;
        let resource = identifier_key(&resource);
        if !bound_requirements.insert(requirement.clone()) {
            return Err(invalid(format!(
                "task instance `{instance}` binds {} requirement `{requirement}` more than once",
                category.name()
            )));
        }
        if !bound_resources.insert(resource.clone()) {
            return Err(invalid(format!(
                "task instance `{instance}` binds system resource `{resource}` more than once"
            )));
        }
        if !declared.contains(&requirement) {
            if other_category.contains(&requirement) {
                return Err(invalid(format!(
                    "task instance `{instance}` binds `{requirement}` as {}, but it is declared {}",
                    category.name(),
                    opposite(category).name()
                )));
            }
            return Err(invalid(format!(
                "task instance `{instance}` has no {} resource requirement `{requirement}`",
                category.name()
            )));
        }
        match resource_categories.get(&resource) {
            Some(existing) if existing != &category => {
                return Err(invalid(format!(
                    "system resource `{resource}` is bound as both {} and {}",
                    existing.name(),
                    category.name()
                )));
            }
            Some(ResourceCategory::Local) => {
                return Err(invalid(format!(
                    "local system resource `{resource}` is bound to more than one task requirement"
                )));
            }
            _ => {
                resource_categories.insert(resource, category);
            }
        }
    }

    if bound_requirements != declared {
        let missing = declared
            .difference(&bound_requirements)
            .cloned()
            .collect::<Vec<_>>();
        return Err(invalid(format!(
            "task instance `{instance}` is missing {} resource bindings: {}",
            category.name(),
            missing.join(", ")
        )));
    }
    Ok(())
}

const fn opposite(category: ResourceCategory) -> ResourceCategory {
    match category {
        ResourceCategory::Local => ResourceCategory::Shared,
        ResourceCategory::Shared => ResourceCategory::Local,
    }
}

fn validate_configuration(
    instance_name: &str,
    instance: &TaskInstance<'_>,
    selection: &TaskSelection,
) -> Result<(), RenderError> {
    let declared = instance
        .definition
        .contract
        .arguments
        .config
        .iter()
        .map(|configuration| (identifier_key(&configuration.name), configuration))
        .collect::<BTreeMap<_, _>>();
    let mut configured = BTreeSet::new();
    for binding in &selection.configuration {
        let name = identifier_key(&identifier(&binding.name, "configuration name")?);
        if !configured.insert(name.clone()) {
            return Err(invalid(format!(
                "task instance `{instance_name}` configures `{name}` more than once"
            )));
        }
        let expected = declared.get(&name).ok_or_else(|| {
            invalid(format!(
                "task instance `{instance_name}` has no configuration `{name}`"
            ))
        })?;
        let actual_type: Type = syn::parse_str(&binding.rust_type).map_err(|error| {
            invalid(format!(
                "task instance `{instance_name}` configuration `{name}` has an invalid type: {error}"
            ))
        })?;
        let expected_type = expected
            .ty
            .as_ref()
            .expect("standalone configuration types were validated during discovery");
        if tokens(&actual_type) != tokens(expected_type) {
            return Err(invalid(format!(
                "task instance `{instance_name}` configuration `{name}` must use declared type `{}`",
                tokens(expected_type)
            )));
        }
        syn::parse_str::<Expr>(&binding.value).map_err(|error| {
            invalid(format!(
                "task instance `{instance_name}` configuration `{name}` has an invalid value expression: {error}"
            ))
        })?;
    }
    let missing = declared
        .keys()
        .filter(|name| !configured.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(invalid(format!(
            "task instance `{instance_name}` is missing configuration bindings: {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

fn validate_spawns(
    instance_name: &str,
    instance: &TaskInstance<'_>,
    selection: &TaskSelection,
    instances: &BTreeMap<String, &TaskInstance<'_>>,
) -> Result<(), RenderError> {
    let declared = instance
        .definition
        .contract
        .arguments
        .spawn
        .iter()
        .map(|spawn| (identifier_key(&spawn.name), spawn))
        .collect::<BTreeMap<_, _>>();
    let mut bound = BTreeSet::new();
    for binding in &selection.spawn {
        let alias = identifier_key(&identifier(&binding.alias, "spawn alias")?);
        if !bound.insert(alias.clone()) {
            return Err(invalid(format!(
                "task instance `{instance_name}` binds spawn alias `{alias}` more than once"
            )));
        }
        let declaration = declared.get(&alias).ok_or_else(|| {
            invalid(format!(
                "task instance `{instance_name}` has no spawn alias `{alias}`"
            ))
        })?;
        let target_name = identifier_key(&identifier(&binding.target, "spawn target")?);
        let target = instances.get(&target_name).ok_or_else(|| {
            invalid(format!(
                "spawn target `{}` is not selected by the composition",
                binding.target
            ))
        })?;
        // A hardware task is entered by its interrupt, so nothing can spawn it.
        if target.definition.contract.kind == TaskKind::Hardware {
            return Err(invalid(format!(
                "task instance `{instance_name}` spawn alias `{alias}` targets hardware task `{}`, which only its interrupt can enter",
                binding.target
            )));
        }
        let outgoing = declaration
            .inputs
            .as_ref()
            .expect("standalone spawn signatures were validated during discovery");
        let incoming = &target.definition.contract.inputs;
        if outgoing.len() != incoming.len() {
            return Err(invalid(format!(
                "task instance `{instance_name}` spawn alias `{alias}` declares {} inputs, but target `{}` accepts {}",
                outgoing.len(),
                binding.target,
                incoming.len()
            )));
        }
    }
    let missing = declared
        .keys()
        .filter(|name| !bound.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(invalid(format!(
            "task instance `{instance_name}` is missing spawn bindings: {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

fn identifier(value: &str, role: &str) -> Result<Ident, RenderError> {
    syn::parse_str(value).map_err(|_| invalid(format!("invalid {role} `{value}`")))
}

fn tokens(value: &Type) -> String {
    value.to_token_stream().to_string()
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}
