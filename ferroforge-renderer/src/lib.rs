//! Renders a FerroForge application declaration as a standalone RTIC project.

pub mod composition;
pub mod dependencies;
pub mod init_check;
mod loader;
pub mod source;
pub mod standalone;
pub mod transplant;

pub use loader::{
    LoadedApplication, LoadedDependency, LoadedMonotonic, LoadedMonotonicSource, LoadedTarget,
    LoadedTask, LoadedTaskConfiguration, LoadedTaskDependency, LoadedTaskImplementation,
    LoadedTaskSpawnBinding, load_application,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
};

use ferroforge::{
    ApplicationDefinition, CompositionDefinition, DependencyCatalogEntry, MonotonicDefinition,
    MonotonicSourceDefinition, TargetDefinition, TaskDefinition,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Attribute, File, Ident, Item, ItemFn, ItemStruct, Path as SynPath, parse_quote,
    visit_mut::{self, VisitMut},
};

const RTIC_VERSION: &str = "2.3.1";
const RTIC_MONOTONICS_VERSION: &str = "2.2.1";
const CORTEX_M_VERSION: &str = "0.7.7";
const CORTEX_M_RT_VERSION: &str = "0.7.6";
const STM32F4XX_HAL_VERSION: &str = "0.23.0";
const DEFMT_RTT_VERSION: &str = "1.3.0";
const PANIC_PROBE_VERSION: &str = "1.0.0";

/// Options which describe the generated Cargo project rather than firmware behavior.
#[derive(Debug, Clone)]
pub struct RenderOptions<'a> {
    pub package_name: &'a str,
    pub output_dir: &'a Path,
}

/// Files written by a successful render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedProject {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub main_source: PathBuf,
    pub memory_layout: PathBuf,
    pub cargo_config: PathBuf,
    pub embed_config: PathBuf,
}

#[derive(Debug, Clone)]
struct ApplicationView {
    manifest_dir: String,
    source_file: String,
    module: String,
    target: Option<LoadedTarget>,
    dispatchers: Vec<String>,
    monotonic: Option<LoadedMonotonic>,
    shared_source: String,
    local_source: String,
    init_source: String,
    tasks: Vec<LoadedTask>,
    dependencies: Vec<LoadedDependency>,
}

impl ApplicationView {
    fn from_definition(application: &ApplicationDefinition) -> Self {
        Self {
            manifest_dir: application.manifest_dir.to_owned(),
            source_file: application.source_file.to_owned(),
            module: application.module.to_owned(),
            target: application.target.map(loaded_target),
            dispatchers: application
                .dispatchers
                .iter()
                .map(|dispatcher| (*dispatcher).to_owned())
                .collect(),
            monotonic: application.monotonic.map(loaded_monotonic),
            shared_source: application.shared_source.to_owned(),
            local_source: application.local_source.to_owned(),
            init_source: application.init_source.to_owned(),
            tasks: application.tasks.iter().map(loaded_task).collect(),
            dependencies: application
                .dependencies
                .iter()
                .map(loaded_dependency)
                .collect(),
        }
    }

    fn from_loaded(application: &LoadedApplication) -> Self {
        Self {
            manifest_dir: application.manifest_dir.clone(),
            source_file: application.source_file.clone(),
            module: application.module.clone(),
            target: Some(application.target.clone()),
            dispatchers: application.dispatchers.clone(),
            monotonic: application.monotonic.clone(),
            shared_source: application.shared_source.clone(),
            local_source: application.local_source.clone(),
            init_source: application.init_source.clone(),
            tasks: application.tasks.clone(),
            dependencies: application.dependencies.clone(),
        }
    }

    fn apply_composition(
        mut self,
        composition: &CompositionDefinition,
    ) -> Result<Self, RenderError> {
        self.tasks = compose_tasks(&self, composition)?;
        self.dispatchers = composition
            .dispatchers
            .iter()
            .map(|dispatcher| (*dispatcher).to_owned())
            .collect();
        Ok(self)
    }
}

fn loaded_target(target: TargetDefinition) -> LoadedTarget {
    LoadedTarget {
        mcu: target.mcu.to_owned(),
        hal: target.hal.to_owned(),
        rust_target: target.rust_target.to_owned(),
        hal_feature: target.hal_feature.to_owned(),
        rtic_monotonics_feature: target.rtic_monotonics_feature.to_owned(),
        flash_origin: target.flash.origin,
        flash_size_bytes: target.flash.size_bytes,
        ram_origin: target.ram.origin,
        ram_size_bytes: target.ram.size_bytes,
    }
}

fn loaded_monotonic(monotonic: MonotonicDefinition) -> LoadedMonotonic {
    LoadedMonotonic {
        name: monotonic.name.to_owned(),
        source: match monotonic.source {
            MonotonicSourceDefinition::SysTick => LoadedMonotonicSource::SysTick,
            MonotonicSourceDefinition::Timer(timer) => {
                LoadedMonotonicSource::Timer(timer.to_owned())
            }
        },
        tick_hz: monotonic.tick_hz,
    }
}

fn loaded_task(task: &TaskDefinition) -> LoadedTask {
    LoadedTask {
        name: task.name.to_owned(),
        priority: task.priority,
        interrupt: task.interrupt.map(str::to_owned),
        implementation: LoadedTaskImplementation {
            source: task.implementation.source.to_owned(),
            shared_resources: task
                .implementation
                .shared_resources
                .iter()
                .map(|resource| (*resource).to_owned())
                .collect(),
            local_resources: task
                .implementation
                .local_resources
                .iter()
                .map(|resource| (*resource).to_owned())
                .collect(),
            config_keys: task
                .implementation
                .config_keys
                .iter()
                .map(|key| (*key).to_owned())
                .collect(),
            spawn_aliases: task
                .implementation
                .spawn_aliases
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect(),
            dependencies: task
                .implementation
                .dependencies
                .iter()
                .map(|dependency| LoadedTaskDependency {
                    id: dependency.id.to_owned(),
                    features: dependency
                        .features
                        .iter()
                        .map(|feature| (*feature).to_owned())
                        .collect(),
                })
                .collect(),
        },
        configuration: task
            .configuration
            .iter()
            .map(|configuration| LoadedTaskConfiguration {
                name: configuration.name.to_owned(),
                rust_type: configuration.rust_type.to_owned(),
                value: configuration.value.to_owned(),
            })
            .collect(),
        spawn_bindings: task
            .spawn_bindings
            .iter()
            .map(|binding| LoadedTaskSpawnBinding {
                alias: binding.alias.to_owned(),
                target: binding.target.to_owned(),
            })
            .collect(),
    }
}

fn loaded_dependency(dependency: &DependencyCatalogEntry) -> LoadedDependency {
    LoadedDependency {
        id: dependency.id.to_owned(),
        package: dependency.package.to_owned(),
        version: dependency.version.to_owned(),
        default_features: dependency.default_features,
        features: dependency
            .features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect(),
    }
}

/// Failure while validating or rendering an application declaration.
#[derive(Debug)]
pub enum RenderError {
    MissingTarget,
    UnsupportedTarget(String),
    MissingDependency(String),
    ConflictingDependency(String),
    InvalidDeclaration(String),
    SourceFileNotFound(Vec<PathBuf>),
    Parse(syn::Error),
    CargoMetadata(String),
    Io(std::io::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTarget => formatter.write_str("rendering requires a target declaration"),
            Self::UnsupportedTarget(target) => {
                write!(
                    formatter,
                    "the RTIC renderer does not support target `{target}`"
                )
            }
            Self::MissingDependency(id) => {
                write!(
                    formatter,
                    "task dependency `{id}` is absent from the registry"
                )
            }
            Self::ConflictingDependency(package) => write!(
                formatter,
                "dependency `{package}` conflicts with a renderer backend dependency"
            ),
            Self::InvalidDeclaration(message) => formatter.write_str(message),
            Self::SourceFileNotFound(candidates) => write!(
                formatter,
                "could not locate declaration source; tried {}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Parse(error) => write!(formatter, "could not parse declaration source: {error}"),
            Self::CargoMetadata(error) => {
                write!(formatter, "could not read Cargo metadata: {error}")
            }
            Self::Io(error) => write!(formatter, "could not write rendered project: {error}"),
        }
    }
}

impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<syn::Error> for RenderError {
    fn from(error: syn::Error) -> Self {
        Self::Parse(error)
    }
}

impl From<std::io::Error> for RenderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CargoDependency {
    version: String,
    default_features: bool,
    features: BTreeSet<String>,
}

impl CargoDependency {
    fn new(version: impl Into<String>, default_features: bool, features: &[&str]) -> Self {
        Self {
            version: version.into(),
            default_features,
            features: features
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
        }
    }
}

/// Renders `application` into `options.output_dir`.
pub fn render(
    application: &ApplicationDefinition,
    options: RenderOptions<'_>,
) -> Result<RenderedProject, RenderError> {
    let application = ApplicationView::from_definition(application);
    render_project(&application, options)
}

/// Renders an application loaded from an embedded source workspace.
pub fn render_loaded(
    application: &LoadedApplication,
    options: RenderOptions<'_>,
) -> Result<RenderedProject, RenderError> {
    let application = ApplicationView::from_loaded(application);
    render_project(&application, options)
}

/// Applies a host composition and renders a complete standalone RTIC project.
pub fn render_composed(
    application: &ApplicationDefinition,
    composition: &CompositionDefinition,
    options: RenderOptions<'_>,
) -> Result<RenderedProject, RenderError> {
    let application =
        ApplicationView::from_definition(application).apply_composition(composition)?;
    render_project(&application, options)
}

/// Applies a host composition to an application loaded from source and renders it.
pub fn render_loaded_composed(
    application: &LoadedApplication,
    composition: &CompositionDefinition,
    options: RenderOptions<'_>,
) -> Result<RenderedProject, RenderError> {
    let application = ApplicationView::from_loaded(application).apply_composition(composition)?;
    render_project(&application, options)
}

fn render_project(
    application: &ApplicationView,
    options: RenderOptions<'_>,
) -> Result<RenderedProject, RenderError> {
    validate_package_name(options.package_name)?;
    let target = validate_application(application)?;

    let firmware_imports = load_firmware_imports(application)?;
    let main_source = render_main(application, &firmware_imports, true)?;
    let dependencies = resolve_dependencies(application)?;
    let manifest = render_manifest(options.package_name, &dependencies);
    let probe_rs_chip = render_probe_rs_chip(target)?;
    let cargo_config = render_cargo_config(&target.rust_target, probe_rs_chip);
    let embed_config = render_embed_config(probe_rs_chip);
    let memory_layout = render_memory_layout(application)?;

    let root = options.output_dir.to_path_buf();
    let source_dir = root.join("src");
    let cargo_dir = root.join(".cargo");
    fs::create_dir_all(&source_dir)?;
    fs::create_dir_all(&cargo_dir)?;

    let manifest_path = root.join("Cargo.toml");
    let main_path = source_dir.join("main.rs");
    let memory_path = root.join("memory.x");
    let config_path = cargo_dir.join("config.toml");
    let embed_path = root.join("Embed.toml");

    fs::write(&manifest_path, manifest)?;
    fs::write(&main_path, main_source)?;
    format_rust_source(&main_path)?;
    fs::write(&memory_path, memory_layout)?;
    fs::write(&config_path, cargo_config)?;
    fs::write(&embed_path, embed_config)?;
    fs::write(root.join(".gitignore"), "/target/\n")?;

    Ok(RenderedProject {
        root,
        manifest: manifest_path,
        main_source: main_path,
        memory_layout: memory_path,
        cargo_config: config_path,
        embed_config: embed_path,
    })
}

fn validate_application(application: &ApplicationView) -> Result<&LoadedTarget, RenderError> {
    let target = application
        .target
        .as_ref()
        .ok_or(RenderError::MissingTarget)?;

    if target.mcu != "STM32F401RET6" || target.hal != "stm32f4xx_hal" {
        return Err(RenderError::UnsupportedTarget(target.mcu.to_owned()));
    }

    Ok(target)
}

fn compose_tasks(
    application: &ApplicationView,
    composition: &CompositionDefinition,
) -> Result<Vec<LoadedTask>, RenderError> {
    let available: BTreeMap<_, _> = application
        .tasks
        .iter()
        .map(|task| (task.name.as_str(), task))
        .collect();
    let selected_names: BTreeSet<_> = composition.tasks.iter().map(|task| task.name).collect();

    if selected_names.len() != composition.tasks.len() {
        return Err(RenderError::InvalidDeclaration(
            "composition selects a task more than once".to_owned(),
        ));
    }

    let mut tasks = Vec::with_capacity(composition.tasks.len());
    for composed in composition.tasks {
        let base = available.get(composed.name).copied().ok_or_else(|| {
            RenderError::InvalidDeclaration(format!(
                "composition selects unknown embedded task `{}`",
                composed.name
            ))
        })?;

        if let Some(interrupt) = composed.interrupt
            && base.interrupt.as_deref() != Some(interrupt)
        {
            return Err(RenderError::InvalidDeclaration(format!(
                "composition binds task `{}` to `{interrupt}`, but its embedded declaration binds {:?}",
                composed.name, base.interrupt
            )));
        }

        if composed.configuration.len() != base.implementation.config_keys.len() {
            return Err(RenderError::InvalidDeclaration(format!(
                "task `{}` requires configuration keys {:?}",
                composed.name, base.implementation.config_keys
            )));
        }

        let mut configured_keys = BTreeSet::new();
        for configured in composed.configuration {
            if !configured_keys.insert(configured.name) {
                return Err(RenderError::InvalidDeclaration(format!(
                    "task `{}` configures `{}` more than once",
                    composed.name, configured.name
                )));
            }

            let declared = base
                .configuration
                .iter()
                .find(|declaration| declaration.name == configured.name)
                .ok_or_else(|| {
                    RenderError::InvalidDeclaration(format!(
                        "task `{}` has no configuration key `{}`",
                        composed.name, configured.name
                    ))
                })?;

            if normalize_type(&declared.rust_type)? != normalize_type(configured.rust_type)? {
                return Err(RenderError::InvalidDeclaration(format!(
                    "task `{}` configuration `{}` must have type `{}`",
                    composed.name, configured.name, declared.rust_type
                )));
            }
        }

        if composed.spawn_bindings.len() != base.implementation.spawn_aliases.len() {
            return Err(RenderError::InvalidDeclaration(format!(
                "task `{}` requires spawn aliases {:?}",
                composed.name, base.implementation.spawn_aliases
            )));
        }

        let mut aliases = BTreeSet::new();
        for binding in composed.spawn_bindings {
            if !aliases.insert(binding.alias) {
                return Err(RenderError::InvalidDeclaration(format!(
                    "task `{}` binds spawn alias `{}` more than once",
                    composed.name, binding.alias
                )));
            }
            if !base
                .implementation
                .spawn_aliases
                .iter()
                .any(|alias| alias == binding.alias)
            {
                return Err(RenderError::InvalidDeclaration(format!(
                    "task `{}` has no spawn alias `{}`",
                    composed.name, binding.alias
                )));
            }
            if !selected_names.contains(binding.target) {
                return Err(RenderError::InvalidDeclaration(format!(
                    "spawn target `{}` is not selected by the composition",
                    binding.target
                )));
            }
        }

        tasks.push(LoadedTask {
            name: base.name.clone(),
            priority: composed.priority,
            interrupt: base.interrupt.clone(),
            implementation: base.implementation.clone(),
            configuration: composed
                .configuration
                .iter()
                .map(|configuration| LoadedTaskConfiguration {
                    name: configuration.name.to_owned(),
                    rust_type: configuration.rust_type.to_owned(),
                    value: configuration.value.to_owned(),
                })
                .collect(),
            spawn_bindings: composed
                .spawn_bindings
                .iter()
                .map(|binding| LoadedTaskSpawnBinding {
                    alias: binding.alias.to_owned(),
                    target: binding.target.to_owned(),
                })
                .collect(),
        });
    }

    let dispatcher_names: BTreeSet<_> = composition.dispatchers.iter().copied().collect();
    if dispatcher_names.len() != composition.dispatchers.len() {
        return Err(RenderError::InvalidDeclaration(
            "composition declares a dispatcher more than once".to_owned(),
        ));
    }

    for dispatcher in composition.dispatchers {
        if tasks
            .iter()
            .any(|task| task.interrupt.as_deref() == Some(*dispatcher))
        {
            return Err(RenderError::InvalidDeclaration(format!(
                "dispatcher `{dispatcher}` is also bound to a hardware task"
            )));
        }

        if matches!(
            application.monotonic.as_ref().map(|monotonic| &monotonic.source),
            Some(LoadedMonotonicSource::Timer(timer)) if timer == dispatcher
        ) {
            return Err(RenderError::InvalidDeclaration(format!(
                "dispatcher `{dispatcher}` is reserved by the monotonic"
            )));
        }
    }

    let software_priorities: BTreeSet<_> = tasks
        .iter()
        .filter(|task| task.interrupt.is_none() && task.priority != 0)
        .map(|task| task.priority)
        .collect();
    if composition.dispatchers.len() < software_priorities.len() {
        return Err(RenderError::InvalidDeclaration(format!(
            "composition provides {} dispatchers for {} software priorities {:?}",
            composition.dispatchers.len(),
            software_priorities.len(),
            software_priorities
        )));
    }

    Ok(tasks)
}

fn normalize_type(source: &str) -> Result<String, RenderError> {
    let ty: syn::Type = syn::parse_str(source)?;
    Ok(quote!(#ty).to_string())
}

fn format_rust_source(path: &Path) -> Result<(), RenderError> {
    let mut candidates = Vec::new();

    if let Some(rustfmt) = env::var_os("RUSTFMT") {
        candidates.push(PathBuf::from(rustfmt));
    }
    candidates.push(PathBuf::from("rustfmt"));

    for home_variable in ["USERPROFILE", "HOME"] {
        if let Some(home) = env::var_os(home_variable) {
            let executable = if cfg!(windows) {
                "rustfmt.exe"
            } else {
                "rustfmt"
            };
            candidates.push(PathBuf::from(home).join(".cargo/bin").join(executable));
        }
    }

    for candidate in candidates {
        match Command::new(&candidate)
            .arg("--edition")
            .arg("2024")
            .arg(path)
            .status()
        {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                return Err(RenderError::InvalidDeclaration(format!(
                    "rustfmt exited with status {status}"
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(RenderError::Io(error)),
        }
    }

    Ok(())
}

fn validate_package_name(name: &str) -> Result<(), RenderError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(RenderError::InvalidDeclaration(format!(
            "`{name}` is not a valid generated Cargo package name"
        )));
    }

    Ok(())
}

fn load_firmware_imports(application: &ApplicationView) -> Result<Vec<Item>, RenderError> {
    let source_path = locate_source_file(application)?;
    let source = fs::read_to_string(source_path)?;
    let file = syn::parse_file(&source)?;
    let mut imports = Vec::new();

    for mut item in file.items {
        let attributes = item_attributes_mut(&mut item);
        let Some(attributes) = attributes else {
            continue;
        };
        let is_firmware = attributes.iter().any(is_firmware_attribute);

        if is_firmware {
            attributes.retain(|attribute| !is_firmware_attribute(attribute));
            imports.push(item);
        }
    }

    Ok(imports)
}

fn locate_source_file(application: &ApplicationView) -> Result<PathBuf, RenderError> {
    let manifest_dir = Path::new(&application.manifest_dir);
    let declared = Path::new(&application.source_file);
    let mut candidates = Vec::new();

    if declared.is_absolute() {
        candidates.push(declared.to_path_buf());
    } else {
        candidates.push(manifest_dir.join(declared));
        if let Some(parent) = manifest_dir.parent() {
            candidates.push(parent.join(declared));
        }
        candidates.push(manifest_dir.join("src").join("main.rs"));
    }

    candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
        .ok_or(RenderError::SourceFileNotFound(candidates))
}

fn item_attributes_mut(item: &mut Item) -> Option<&mut Vec<Attribute>> {
    match item {
        Item::Const(item) => Some(&mut item.attrs),
        Item::Enum(item) => Some(&mut item.attrs),
        Item::ExternCrate(item) => Some(&mut item.attrs),
        Item::Fn(item) => Some(&mut item.attrs),
        Item::ForeignMod(item) => Some(&mut item.attrs),
        Item::Impl(item) => Some(&mut item.attrs),
        Item::Macro(item) => Some(&mut item.attrs),
        Item::Mod(item) => Some(&mut item.attrs),
        Item::Static(item) => Some(&mut item.attrs),
        Item::Struct(item) => Some(&mut item.attrs),
        Item::Trait(item) => Some(&mut item.attrs),
        Item::TraitAlias(item) => Some(&mut item.attrs),
        Item::Type(item) => Some(&mut item.attrs),
        Item::Union(item) => Some(&mut item.attrs),
        Item::Use(item) => Some(&mut item.attrs),
        Item::Verbatim(_) | _ => None,
    }
}

fn is_firmware_attribute(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "firmware")
}

fn render_main(
    application: &ApplicationView,
    firmware_imports: &[Item],
    include_crate_attributes: bool,
) -> Result<String, RenderError> {
    let target = application
        .target
        .as_ref()
        .ok_or(RenderError::MissingTarget)?;
    let hal: SynPath = syn::parse_str(&target.hal)?;
    let app_module = Ident::new(&application.module, Span::call_site());
    let dispatchers: Vec<Ident> = application
        .dispatchers
        .iter()
        .map(|dispatcher| Ident::new(dispatcher, Span::call_site()))
        .collect();

    let mut shared: ItemStruct = syn::parse_str(&application.shared_source)?;
    shared.attrs.insert(0, parse_quote!(#[shared]));
    let mut local: ItemStruct = syn::parse_str(&application.local_source)?;
    local.attrs.insert(0, parse_quote!(#[local]));
    let mut init: ItemFn = syn::parse_str(&application.init_source)?;
    rewrite_config_paths(&mut init, application);
    init.attrs.insert(0, parse_quote!(#[init]));

    let task_items = application
        .tasks
        .iter()
        .map(|task| render_task(task, application))
        .collect::<Result<Vec<_>, _>>()?;
    let config_constants = render_config_constants(application)?;
    let monotonic = render_monotonic(application)?;

    let crate_attributes = include_crate_attributes.then(|| {
        quote! {
            #![no_std]
            #![no_main]
        }
    });

    let file: File = syn::parse2(quote! {
        #crate_attributes

        use defmt_rtt as _;
        use panic_probe as _;
        #(#firmware_imports)*

        #monotonic

        #[rtic::app(device = #hal::pac, dispatchers = [#(#dispatchers),*])]
        mod #app_module {
            use super::*;
            use #hal::prelude::*;

            #config_constants
            #shared
            #local
            #init
            #(#task_items)*
        }
    })?;

    Ok(prettyplease::unparse(&file))
}

fn render_monotonic(application: &ApplicationView) -> Result<TokenStream, RenderError> {
    let Some(monotonic) = application.monotonic.as_ref() else {
        return Ok(TokenStream::new());
    };
    let name = Ident::new(&monotonic.name, Span::call_site());
    let tick_hz = monotonic.tick_hz;

    Ok(match &monotonic.source {
        LoadedMonotonicSource::SysTick => quote! {
            use rtic_monotonics::systick::prelude::*;
            systick_monotonic!(#name, #tick_hz);
        },
        LoadedMonotonicSource::Timer(timer) => {
            let timer = timer.to_ascii_lowercase();
            let macro_name = format_ident!("stm32_{}_monotonic", timer);

            quote! {
                use rtic_monotonics::stm32::prelude::*;
                #macro_name!(#name, #tick_hz);
            }
        }
    })
}

fn render_config_constants(application: &ApplicationView) -> Result<TokenStream, RenderError> {
    let constants = application
        .tasks
        .iter()
        .flat_map(|task| {
            task.configuration.iter().map(|configuration| {
                let name = config_constant_ident(&task.name, &configuration.name);
                let ty: syn::Type = syn::parse_str(&configuration.rust_type)?;
                let value: syn::Expr = syn::parse_str(&configuration.value)?;

                Ok::<_, syn::Error>(quote! {
                    const #name: #ty = #value;
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(quote! {
        #(#constants)*
    })
}

fn config_constant_ident(task: &str, configuration: &str) -> Ident {
    format_ident!(
        "{}_{}",
        task.to_ascii_uppercase(),
        configuration.to_ascii_uppercase()
    )
}

fn render_task(
    task: &LoadedTask,
    application: &ApplicationView,
) -> Result<TokenStream, RenderError> {
    let mut function: ItemFn = syn::parse_str(&task.implementation.source)?;
    function.vis = syn::Visibility::Inherited;
    rewrite_config_paths(&mut function, application);
    let priority = syn::LitInt::new(&task.priority.to_string(), Span::call_site());
    let shared: Vec<Ident> = task
        .implementation
        .shared_resources
        .iter()
        .map(|resource| Ident::new(resource, Span::call_site()))
        .collect();
    let local: Vec<Ident> = task
        .implementation
        .local_resources
        .iter()
        .map(|resource| Ident::new(resource, Span::call_site()))
        .collect();
    let interrupt = task
        .interrupt
        .as_ref()
        .map(|interrupt| Ident::new(interrupt, Span::call_site()));

    let attribute = match (interrupt, shared.is_empty(), local.is_empty()) {
        (Some(interrupt), true, true) => quote!(#[task(binds = #interrupt, priority = #priority)]),
        (Some(interrupt), false, true) => {
            quote!(#[task(binds = #interrupt, priority = #priority, shared = [#(#shared),*])])
        }
        (Some(interrupt), true, false) => {
            quote!(#[task(binds = #interrupt, priority = #priority, local = [#(#local),*])])
        }
        (Some(interrupt), false, false) => quote! {
            #[task(
                binds = #interrupt,
                priority = #priority,
                shared = [#(#shared),*],
                local = [#(#local),*]
            )]
        },
        (None, true, true) => quote!(#[task(priority = #priority)]),
        (None, false, true) => {
            quote!(#[task(priority = #priority, shared = [#(#shared),*])])
        }
        (None, true, false) => {
            quote!(#[task(priority = #priority, local = [#(#local),*])])
        }
        (None, false, false) => quote! {
            #[task(
                priority = #priority,
                shared = [#(#shared),*],
                local = [#(#local),*]
            )]
        },
    };

    Ok(quote! {
        #attribute
        #function
    })
}

fn rewrite_config_paths(function: &mut ItemFn, application: &ApplicationView) {
    let mut rewriter = ConfigPathRewriter {
        tasks: application
            .tasks
            .iter()
            .map(|task| task.name.clone())
            .collect(),
        constants: application
            .tasks
            .iter()
            .flat_map(|task| {
                task.configuration.iter().map(|configuration| {
                    (
                        format!(
                            "{} :: Config :: {}",
                            task.name,
                            configuration.name.to_ascii_uppercase()
                        ),
                        config_constant_ident(&task.name, &configuration.name).to_string(),
                    )
                })
            })
            .collect(),
    };
    rewriter.visit_item_fn_mut(function);
}

struct ConfigPathRewriter {
    tasks: BTreeSet<String>,
    constants: Vec<(String, String)>,
}

impl VisitMut for ConfigPathRewriter {
    fn visit_path_mut(&mut self, path: &mut SynPath) {
        let segments: Vec<_> = path.segments.iter().collect();

        if path.leading_colon.is_none()
            && segments.len() == 3
            && self.tasks.contains(segments[0].ident.to_string().as_str())
            && segments[1].ident == "Config"
        {
            let task = &segments[0].ident;
            let constant = &segments[2].ident;
            let constant = config_constant_ident(&task.to_string(), &constant.to_string());
            *path = parse_quote!(#constant);
            return;
        }

        visit_mut::visit_path_mut(self, path);
    }

    fn visit_macro_mut(&mut self, item: &mut syn::Macro) {
        let mut tokens = item.tokens.to_string();

        for (source, replacement) in &self.constants {
            tokens = tokens.replace(source, replacement);
        }

        item.tokens = tokens
            .parse()
            .expect("rewriting a Rust path preserves a valid token stream");
    }
}

fn resolve_dependencies(
    application: &ApplicationView,
) -> Result<BTreeMap<String, CargoDependency>, RenderError> {
    let target = application
        .target
        .as_ref()
        .ok_or(RenderError::MissingTarget)?;
    let mut dependencies = BTreeMap::new();

    insert_backend_dependency(
        &mut dependencies,
        "cortex-m",
        CargoDependency::new(CORTEX_M_VERSION, true, &[]),
    );
    insert_backend_dependency(
        &mut dependencies,
        "cortex-m-rt",
        CargoDependency::new(CORTEX_M_RT_VERSION, true, &[]),
    );
    insert_backend_dependency(
        &mut dependencies,
        "defmt-rtt",
        CargoDependency::new(DEFMT_RTT_VERSION, true, &[]),
    );
    insert_backend_dependency(
        &mut dependencies,
        "panic-probe",
        CargoDependency::new(PANIC_PROBE_VERSION, true, &["print-defmt"]),
    );
    insert_backend_dependency(
        &mut dependencies,
        "rtic",
        CargoDependency::new(RTIC_VERSION, false, &["thumbv7-backend"]),
    );

    let monotonic_features = match application.monotonic.as_ref().map(|mono| &mono.source) {
        Some(LoadedMonotonicSource::SysTick) => vec!["cortex-m-systick".to_owned()],
        Some(LoadedMonotonicSource::Timer(timer)) => vec![
            target.rtic_monotonics_feature.to_owned(),
            format!("stm32_{}", timer.to_ascii_lowercase()),
        ],
        None => Vec::new(),
    };
    insert_backend_dependency(
        &mut dependencies,
        "rtic-monotonics",
        CargoDependency {
            version: RTIC_MONOTONICS_VERSION.to_owned(),
            default_features: false,
            features: monotonic_features.into_iter().collect(),
        },
    );
    insert_backend_dependency(
        &mut dependencies,
        "stm32f4xx-hal",
        CargoDependency::new(STM32F4XX_HAL_VERSION, false, &[&target.hal_feature]),
    );

    let catalog: BTreeMap<_, _> = application
        .dependencies
        .iter()
        .map(|dependency| (dependency.id.as_str(), dependency))
        .collect();

    for task in &application.tasks {
        for requirement in &task.implementation.dependencies {
            let definition = catalog
                .get(requirement.id.as_str())
                .copied()
                .ok_or_else(|| RenderError::MissingDependency(requirement.id.clone()))?;
            insert_task_dependency(&mut dependencies, definition, &requirement.features)?;
        }
    }

    Ok(dependencies)
}

fn insert_backend_dependency(
    dependencies: &mut BTreeMap<String, CargoDependency>,
    package: &str,
    dependency: CargoDependency,
) {
    dependencies.insert(package.to_owned(), dependency);
}

fn insert_task_dependency(
    dependencies: &mut BTreeMap<String, CargoDependency>,
    definition: &LoadedDependency,
    requested_features: &[String],
) -> Result<(), RenderError> {
    if let Some(existing) = dependencies.get_mut(&definition.package) {
        if existing.version != definition.version
            || existing.default_features != definition.default_features
        {
            return Err(RenderError::ConflictingDependency(
                definition.package.clone(),
            ));
        }
        existing
            .features
            .extend(definition.features.iter().cloned());
        existing.features.extend(requested_features.iter().cloned());
        return Ok(());
    }

    let mut dependency = CargoDependency::new(
        definition.version.clone(),
        definition.default_features,
        &definition
            .features
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    dependency
        .features
        .extend(requested_features.iter().cloned());
    dependencies.insert(definition.package.clone(), dependency);
    Ok(())
}

fn render_manifest(package_name: &str, dependencies: &BTreeMap<String, CargoDependency>) -> String {
    let mut manifest = format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\nbuild = false\n\n[[bin]]\nname = \"{package_name}\"\npath = \"src/main.rs\"\ntest = false\nbench = false\n\n[dependencies]\n"
    );

    for (package, dependency) in dependencies {
        let features = dependency
            .features
            .iter()
            .map(|feature| format!("\"{feature}\""))
            .collect::<Vec<_>>()
            .join(", ");

        if dependency.default_features && dependency.features.is_empty() {
            manifest.push_str(&format!("{package} = \"{}\"\n", dependency.version));
        } else {
            manifest.push_str(&format!(
                "{package} = {{ version = \"{}\", default-features = {}, features = [{}] }}\n",
                dependency.version, dependency.default_features, features
            ));
        }
    }

    manifest.push_str(
        "\n[profile.release]\ncodegen-units = 1\ndebug = 2\nlto = true\nopt-level = \"s\"\n\n[workspace]\n",
    );
    manifest
}

fn render_probe_rs_chip(target: &LoadedTarget) -> Result<&'static str, RenderError> {
    match target.mcu.as_str() {
        "STM32F401RET6" => Ok("STM32F401RE"),
        _ => Err(RenderError::UnsupportedTarget(target.mcu.to_owned())),
    }
}

fn render_cargo_config(rust_target: &str, probe_rs_chip: &str) -> String {
    format!(
        "[build]\ntarget = \"{rust_target}\"\n\n[target.{rust_target}]\nrunner = \"probe-rs run --chip {probe_rs_chip}\"\nrustflags = [\n    \"-C\", \"link-arg=-L.\",\n    \"-C\", \"link-arg=-Tlink.x\",\n    \"-C\", \"link-arg=-Tdefmt.x\",\n]\n\n[env]\nDEFMT_LOG = \"info\"\n"
    )
}

fn render_embed_config(probe_rs_chip: &str) -> String {
    format!(
        "[default.general]\nchip = \"{probe_rs_chip}\"\n\n[default.rtt]\nenabled = true\nup_channels = [\n    {{ channel = 0, mode = \"BlockIfFull\", format = \"Defmt\" }},\n]\n"
    )
}

fn render_memory_layout(application: &ApplicationView) -> Result<String, RenderError> {
    let target = application
        .target
        .as_ref()
        .ok_or(RenderError::MissingTarget)?;
    let text_offset = match target.mcu.as_str() {
        // The F401 vector table ends at 0x194. Start code at the next 8-byte
        // boundary to satisfy current rust-lld section-alignment checks.
        "STM32F401RET6" => 0x198,
        _ => return Err(RenderError::UnsupportedTarget(target.mcu.to_owned())),
    };

    Ok(format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010X}, LENGTH = {}K\n  RAM   : ORIGIN = {:#010X}, LENGTH = {}K\n}}\n\n_stext = ORIGIN(FLASH) + {:#X};\n",
        target.flash_origin,
        target.flash_size_bytes / 1024,
        target.ram_origin,
        target.ram_size_bytes / 1024,
        text_offset,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferroforge::{
        ComposedTaskDefinition, CompositionDefinition, MemoryRegionDefinition, MonotonicDefinition,
        TargetDefinition, TaskConfigurationDefinition, TaskImplementationDefinition,
    };

    const TARGET: TargetDefinition = TargetDefinition {
        mcu: "STM32F401RET6",
        hal: "stm32f4xx_hal",
        rust_target: "thumbv7em-none-eabihf",
        hal_feature: "stm32f401",
        rtic_monotonics_feature: "stm32f401re",
        flash: MemoryRegionDefinition {
            origin: 0x0800_0000,
            size_bytes: 512 * 1024,
        },
        ram: MemoryRegionDefinition {
            origin: 0x2000_0000,
            size_bytes: 96 * 1024,
        },
    };

    const TASK: TaskDefinition = TaskDefinition {
        name: "blink",
        priority: 1,
        interrupt: None,
        implementation: TaskImplementationDefinition {
            source: "async fn blink(_cx: blink::Context) { let _ = blink::Config::PERIOD_MS; defmt::info!(\"{}\", blink::Config::PERIOD_MS); }",
            shared_resources: &[],
            local_resources: &[],
            config_keys: &["period_ms"],
            spawn_aliases: &[],
            dependencies: &[],
        },
        configuration: &[ferroforge::TaskConfigurationDefinition {
            name: "period_ms",
            rust_type: "u64",
            value: "500",
        }],
        spawn_bindings: &[],
    };

    const APPLICATION: ApplicationDefinition = ApplicationDefinition {
        manifest_dir: env!("CARGO_MANIFEST_DIR"),
        source_file: "src/lib.rs",
        module: "app",
        target: Some(TARGET),
        dispatchers: &["USART1"],
        monotonic: Some(MonotonicDefinition {
            name: "Mono",
            source: MonotonicSourceDefinition::Timer("TIM5"),
            tick_hz: 1_000,
        }),
        shared_source: "struct Shared {}",
        local_source: "struct Local {}",
        init_source: "fn init(_cx: init::Context) -> (Shared, Local) { (Shared {}, Local {}) }",
        tasks: &[TASK],
        dependencies: &[],
    };

    const COMPOSITION: CompositionDefinition = CompositionDefinition {
        dispatchers: &["USART2"],
        tasks: &[ComposedTaskDefinition {
            name: "blink",
            priority: 3,
            interrupt: None,
            configuration: &[TaskConfigurationDefinition {
                name: "period_ms",
                rust_type: "u64",
                value: "250",
            }],
            spawn_bindings: &[],
        }],
    };

    #[test]
    fn rewrites_mock_configuration_paths_to_plain_constants() {
        let mut function: ItemFn = syn::parse_str(TASK.implementation.source).unwrap();
        let application = ApplicationView::from_definition(&APPLICATION);
        rewrite_config_paths(&mut function, &application);
        let source = quote!(#function).to_string();

        assert_eq!(source.matches("BLINK_PERIOD_MS").count(), 2);
        assert!(!source.contains("blink :: Config"));
        assert!(!source.contains("ferroforge"));
    }

    #[test]
    fn emits_target_memory_layout() {
        let application = ApplicationView::from_definition(&APPLICATION);
        let memory = render_memory_layout(&application).unwrap();

        assert!(memory.contains("ORIGIN = 0x08000000, LENGTH = 512K"));
        assert!(memory.contains("ORIGIN = 0x20000000, LENGTH = 96K"));
    }

    #[test]
    fn emits_probe_rs_chip_configuration() {
        let target = loaded_target(TARGET);
        let chip = render_probe_rs_chip(&target).unwrap();

        assert_eq!(chip, "STM32F401RE");
        let cargo_config = render_cargo_config(TARGET.rust_target, chip);
        assert!(cargo_config.contains("--chip STM32F401RE"));
        assert!(cargo_config.contains("DEFMT_LOG = \"info\""));
        assert_eq!(
            render_embed_config(chip),
            "[default.general]\nchip = \"STM32F401RE\"\n\n[default.rtt]\nenabled = true\nup_channels = [\n    { channel = 0, mode = \"BlockIfFull\", format = \"Defmt\" },\n]\n"
        );
    }

    #[test]
    fn disables_test_and_bench_harnesses_for_the_firmware_binary() {
        let manifest = render_manifest("firmware", &BTreeMap::new());

        assert!(manifest.contains("[[bin]]"));
        assert!(manifest.contains("build = false"));
        assert!(manifest.contains("test = false"));
        assert!(manifest.contains("bench = false"));
    }

    #[test]
    fn host_composition_overrides_embedded_check_values() {
        let base = ApplicationView::from_definition(&APPLICATION);
        let tasks = compose_tasks(&base, &COMPOSITION).unwrap();
        let application = base.apply_composition(&COMPOSITION).unwrap();
        let source = render_main(&application, &[], false).unwrap();

        assert_eq!(tasks[0].priority, 3);
        assert_eq!(tasks[0].configuration[0].value, "250");
        assert_eq!(application.dispatchers, ["USART2"]);
        assert!(!source.contains("#![no_std]"));
        assert!(!source.contains("#![no_main]"));
        assert!(source.contains("const BLINK_PERIOD_MS: u64 = 250"));
        assert!(source.contains("priority = 3"));
        assert!(source.contains("dispatchers = [USART2]"));
        assert!(!source.contains("__ferroforge_config"));
    }

    #[test]
    fn renders_composition_as_a_standalone_project() {
        let output_dir = std::env::temp_dir().join(format!(
            "ferroforge-renderer-composed-project-test-{}",
            std::process::id()
        ));
        if output_dir.exists() {
            fs::remove_dir_all(&output_dir).unwrap();
        }

        let rendered = render_composed(
            &APPLICATION,
            &COMPOSITION,
            RenderOptions {
                package_name: "composed-firmware",
                output_dir: &output_dir,
            },
        )
        .unwrap();
        let source = fs::read_to_string(&rendered.main_source).unwrap();

        assert!(source.contains("#![no_std]"));
        assert!(source.contains("#![no_main]"));
        assert!(source.contains("const BLINK_PERIOD_MS: u64 = 250"));
        assert!(source.contains("priority = 3"));
        assert!(source.contains("dispatchers = [USART2]"));
        assert!(!source.contains("__ferroforge_config"));
        assert!(!output_dir.join("build.rs").exists());
        assert_eq!(
            fs::read_to_string(&rendered.embed_config).unwrap(),
            "[default.general]\nchip = \"STM32F401RE\"\n\n[default.rtt]\nenabled = true\nup_channels = [\n    { channel = 0, mode = \"BlockIfFull\", format = \"Defmt\" },\n]\n"
        );

        fs::remove_dir_all(output_dir).unwrap();
    }
}
