use std::{collections::BTreeMap, fs, path::Path};

use ferroforge_contracts::LegacyTaskArguments as TaskArgumentsInput;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Expr, Ident, Item, ItemFn, ItemMacro, ItemStruct, LitBool, LitInt, LitStr,
    Path as SynPath, Token, Type, braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::{RenderError, normalize_type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedApplication {
    pub manifest_dir: String,
    pub source_file: String,
    pub module: String,
    pub target: LoadedTarget,
    pub dispatchers: Vec<String>,
    pub monotonic: Option<LoadedMonotonic>,
    pub shared_source: String,
    pub local_source: String,
    pub init_source: String,
    pub tasks: Vec<LoadedTask>,
    pub dependencies: Vec<LoadedDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTarget {
    pub mcu: String,
    pub hal: String,
    pub rust_target: String,
    pub hal_feature: String,
    pub rtic_monotonics_feature: String,
    pub flash_origin: u32,
    pub flash_size_bytes: u32,
    pub ram_origin: u32,
    pub ram_size_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedMonotonicSource {
    SysTick,
    Timer(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedMonotonic {
    pub name: String,
    pub source: LoadedMonotonicSource,
    pub tick_hz: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTask {
    pub name: String,
    pub priority: u8,
    pub interrupt: Option<String>,
    pub implementation: LoadedTaskImplementation,
    pub configuration: Vec<LoadedTaskConfiguration>,
    pub spawn_bindings: Vec<LoadedTaskSpawnBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTaskImplementation {
    pub source: String,
    pub shared_resources: Vec<String>,
    pub local_resources: Vec<String>,
    pub config_keys: Vec<String>,
    pub spawn_aliases: Vec<String>,
    pub dependencies: Vec<LoadedTaskDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTaskDependency {
    pub id: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTaskConfiguration {
    pub name: String,
    pub rust_type: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedTaskSpawnBinding {
    pub alias: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedDependency {
    pub id: String,
    pub package: String,
    pub version: String,
    pub default_features: bool,
    pub features: Vec<String>,
}

pub fn load_application(workspace: &Path) -> Result<LoadedApplication, RenderError> {
    let workspace = workspace.canonicalize()?;
    let source_file = workspace.join("src/lib.rs");
    let source = fs::read_to_string(&source_file)?;
    let file = syn::parse_file(&source)?;
    let app_macro = find_macro(&file.items, "app")?;
    let mut declaration: AppDeclarationInput = syn::parse2(app_macro.mac.tokens.clone())?;

    strip_marker(&mut declaration.shared.attrs, "shared")?;
    strip_marker(&mut declaration.local.attrs, "local")?;
    strip_marker(&mut declaration.init.attrs, "init")?;

    let tasks_file = workspace
        .join("src")
        .join(format!("{}.rs", declaration.tasks_module));
    let implementations = load_task_implementations(&tasks_file)?;
    let tasks = declaration
        .tasks
        .into_iter()
        .map(|task| load_task(task, &implementations))
        .collect::<Result<Vec<_>, _>>()?;

    let dependencies = if let Some(registry) = declaration.dependency_registry {
        let module = registry
            .segments
            .last()
            .ok_or_else(|| invalid("dependency registry path is empty"))?
            .ident
            .to_string();
        load_dependencies(&workspace.join("src").join(format!("{module}.rs")))?
    } else {
        Vec::new()
    };

    let target = load_target(declaration.target)?;
    let monotonic = declaration.monotonic.map(load_monotonic).transpose()?;
    let shared_source = declaration.shared.into_token_stream().to_string();
    let local_source = declaration.local.into_token_stream().to_string();
    let init_source = declaration.init.into_token_stream().to_string();

    Ok(LoadedApplication {
        manifest_dir: workspace.to_string_lossy().into_owned(),
        source_file: "src/lib.rs".to_owned(),
        module: declaration.app_module.to_string(),
        target,
        dispatchers: identifiers(declaration.dispatchers),
        monotonic,
        shared_source,
        local_source,
        init_source,
        tasks,
        dependencies,
    })
}

fn load_target(target: TargetInput) -> Result<LoadedTarget, RenderError> {
    let mcu = target.mcu.to_string();
    let hal = target.hal.to_token_stream().to_string().replace(' ', "");

    match (mcu.as_str(), hal.as_str()) {
        ("STM32F401RET6", "stm32f4xx_hal") => Ok(LoadedTarget {
            mcu,
            hal,
            rust_target: "thumbv7em-none-eabihf".to_owned(),
            hal_feature: "stm32f401".to_owned(),
            rtic_monotonics_feature: "stm32f401re".to_owned(),
            flash_origin: 0x0800_0000,
            flash_size_bytes: 512 * 1024,
            ram_origin: 0x2000_0000,
            ram_size_bytes: 96 * 1024,
        }),
        _ => Err(RenderError::UnsupportedTarget(mcu)),
    }
}

fn load_monotonic(monotonic: MonotonicInput) -> Result<LoadedMonotonic, RenderError> {
    let tick_hz = monotonic.tick_hz.base10_parse::<u32>()?;
    if tick_hz == 0 {
        return Err(invalid(
            "monotonic tick frequency must be greater than zero",
        ));
    }

    let source = match monotonic.source {
        MonotonicSourceInput::SysTick => LoadedMonotonicSource::SysTick,
        MonotonicSourceInput::Timer(timer) => LoadedMonotonicSource::Timer(timer.to_string()),
    };

    Ok(LoadedMonotonic {
        name: monotonic.name.to_string(),
        source,
        tick_hz,
    })
}

fn load_task(
    task: AppTaskInput,
    implementations: &BTreeMap<String, LoadedTaskImplementation>,
) -> Result<LoadedTask, RenderError> {
    let name = task.name.to_string();
    let implementation = implementations
        .get(&name)
        .cloned()
        .ok_or_else(|| invalid(format!("task `{name}` has no #[task] implementation")))?;

    Ok(LoadedTask {
        name,
        priority: task.priority.base10_parse::<u8>()?,
        interrupt: task.binds.map(|interrupt| interrupt.to_string()),
        implementation,
        configuration: task
            .configurations
            .into_iter()
            .map(|configuration| {
                Ok(LoadedTaskConfiguration {
                    name: configuration.name.to_string(),
                    rust_type: normalize_type(&configuration.ty.to_token_stream().to_string())?,
                    value: configuration.value.to_token_stream().to_string(),
                })
            })
            .collect::<Result<Vec<_>, RenderError>>()?,
        spawn_bindings: task
            .spawn_bindings
            .into_iter()
            .map(|binding| LoadedTaskSpawnBinding {
                alias: binding.alias.to_string(),
                target: binding.target.to_string(),
            })
            .collect(),
    })
}

fn load_task_implementations(
    path: &Path,
) -> Result<BTreeMap<String, LoadedTaskImplementation>, RenderError> {
    let source = fs::read_to_string(path)?;
    let file = syn::parse_file(&source)?;
    let mut implementations = BTreeMap::new();

    for item in file.items {
        let Item::Fn(mut function) = item else {
            continue;
        };
        let Some(index) = function
            .attrs
            .iter()
            .position(|attribute| path_ends_with(attribute.path(), "task"))
        else {
            continue;
        };
        let attribute = function.attrs.remove(index);
        let arguments: TaskArgumentsInput = attribute.parse_args()?;
        let name = function.sig.ident.to_string();
        let implementation = LoadedTaskImplementation {
            source: quote!(#function).to_string(),
            shared_resources: identifiers(arguments.shared),
            local_resources: identifiers(arguments.local),
            config_keys: identifiers(arguments.configs),
            spawn_aliases: identifiers(arguments.spawns),
            dependencies: arguments
                .dependencies
                .into_iter()
                .map(|dependency| LoadedTaskDependency {
                    id: dependency.id.to_string(),
                    features: strings(dependency.features),
                })
                .collect(),
        };

        if implementations
            .insert(name.clone(), implementation)
            .is_some()
        {
            return Err(invalid(format!(
                "task `{name}` is implemented more than once"
            )));
        }
    }

    Ok(implementations)
}

fn load_dependencies(path: &Path) -> Result<Vec<LoadedDependency>, RenderError> {
    let source = fs::read_to_string(path)?;
    let file = syn::parse_file(&source)?;
    let registry = find_macro(&file.items, "dependency_registry")?;
    let registry: DependencyRegistryInput = syn::parse2(registry.mac.tokens.clone())?;

    Ok(registry
        .entries
        .into_iter()
        .map(|entry| LoadedDependency {
            id: entry.id.to_string(),
            package: entry.package.value(),
            version: entry.version.value(),
            default_features: entry.default_features.value,
            features: strings(entry.features),
        })
        .collect())
}

fn find_macro<'a>(items: &'a [Item], name: &str) -> Result<&'a ItemMacro, RenderError> {
    items
        .iter()
        .find_map(|item| match item {
            Item::Macro(item) if path_ends_with(&item.mac.path, name) => Some(item),
            _ => None,
        })
        .ok_or_else(|| invalid(format!("could not find `{name}!` declaration")))
}

fn strip_marker(attributes: &mut Vec<Attribute>, marker: &str) -> Result<(), RenderError> {
    let matching: Vec<_> = attributes
        .iter()
        .enumerate()
        .filter(|(_, attribute)| path_ends_with(attribute.path(), marker))
        .map(|(index, _)| index)
        .collect();

    if matching.len() != 1 {
        return Err(invalid(format!(
            "expected exactly one `#[{marker}]` attribute"
        )));
    }
    attributes.remove(matching[0]);
    Ok(())
}

fn path_ends_with(path: &SynPath, name: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

fn identifiers(values: Vec<Ident>) -> Vec<String> {
    values.into_iter().map(|value| value.to_string()).collect()
}

fn strings(values: Vec<LitStr>) -> Vec<String> {
    values.into_iter().map(|value| value.value()).collect()
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}

struct AppDeclarationInput {
    dependency_registry: Option<SynPath>,
    target: TargetInput,
    dispatchers: Vec<Ident>,
    monotonic: Option<MonotonicInput>,
    app_module: Ident,
    tasks_module: Ident,
    shared: ItemStruct,
    local: ItemStruct,
    init: ItemFn,
    tasks: Vec<AppTaskInput>,
}

impl Parse for AppDeclarationInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let dependency_registry =
            parse_optional_assignment::<SynPath>(input, "dependency_registry")?;

        let target_key: Ident = input.parse()?;
        if target_key != "target" {
            return Err(syn::Error::new(target_key.span(), "expected `target`"));
        }
        input.parse::<Token![=]>()?;
        let target = input.parse()?;
        input.parse::<Token![,]>()?;

        let dispatchers_key: Ident = input.parse()?;
        if dispatchers_key != "dispatchers" {
            return Err(syn::Error::new(
                dispatchers_key.span(),
                "expected `dispatchers`",
            ));
        }
        input.parse::<Token![=]>()?;
        let dispatcher_body;
        bracketed!(dispatcher_body in input);
        let dispatchers = Punctuated::<Ident, Token![,]>::parse_terminated(&dispatcher_body)?
            .into_iter()
            .collect();
        input.parse::<Token![,]>()?;

        let monotonic = parse_optional_assignment::<MonotonicInput>(input, "monotonic")?;

        input.parse::<Token![mod]>()?;
        let app_module = input.parse()?;
        let app_body;
        braced!(app_body in input);
        app_body.parse::<Token![mod]>()?;
        let tasks_module = app_body.parse()?;
        app_body.parse::<Token![;]>()?;
        let shared = app_body.parse()?;
        let local = app_body.parse()?;
        let init = app_body.parse()?;
        let tasks = Punctuated::<AppTaskInput, Token![,]>::parse_terminated(&app_body)?
            .into_iter()
            .collect();

        if !input.is_empty() {
            return Err(input.error("unexpected tokens after app declaration"));
        }

        Ok(Self {
            dependency_registry,
            target,
            dispatchers,
            monotonic,
            app_module,
            tasks_module,
            shared,
            local,
            init,
            tasks,
        })
    }
}

fn parse_optional_assignment<T: Parse>(
    input: ParseStream<'_>,
    expected: &str,
) -> syn::Result<Option<T>> {
    if !input.peek(Ident) {
        return Ok(None);
    }
    let lookahead = input.fork();
    let key: Ident = lookahead.parse()?;
    if key != expected || !lookahead.peek(Token![=]) {
        return Ok(None);
    }

    input.parse::<Ident>()?;
    input.parse::<Token![=]>()?;
    let value = input.parse()?;
    input.parse::<Token![,]>()?;
    Ok(Some(value))
}

struct TargetInput {
    mcu: Ident,
    hal: SynPath,
}

impl Parse for TargetInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let body;
        braced!(body in input);
        let mut mcu = None;
        let mut hal = None;

        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "mcu" => mcu = Some(body.parse()?),
                "hal" => hal = Some(body.parse()?),
                _ => return Err(syn::Error::new(key.span(), "expected `mcu` or `hal`")),
            }
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            mcu: mcu.ok_or_else(|| body.error("target requires `mcu`"))?,
            hal: hal.ok_or_else(|| body.error("target requires `hal`"))?,
        })
    }
}

enum MonotonicSourceInput {
    SysTick,
    Timer(Ident),
}

struct MonotonicInput {
    name: Ident,
    source: MonotonicSourceInput,
    tick_hz: LitInt,
}

impl Parse for MonotonicInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        let body;
        braced!(body in input);
        let mut source = None;
        let mut tick_hz = None;

        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "source" => {
                    let source_name: Ident = body.parse()?;
                    source = Some(match source_name.to_string().as_str() {
                        "SysTick" => MonotonicSourceInput::SysTick,
                        "Timer" => {
                            let timer_body;
                            parenthesized!(timer_body in body);
                            MonotonicSourceInput::Timer(timer_body.parse()?)
                        }
                        _ => {
                            return Err(syn::Error::new(
                                source_name.span(),
                                "expected `SysTick` or `Timer(TIMx)`",
                            ));
                        }
                    });
                }
                "tick_hz" => tick_hz = Some(body.parse()?),
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `source` or `tick_hz`",
                    ));
                }
            }
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            name,
            source: source.ok_or_else(|| body.error("monotonic requires `source`"))?,
            tick_hz: tick_hz.ok_or_else(|| body.error("monotonic requires `tick_hz`"))?,
        })
    }
}

struct AppTaskInput {
    name: Ident,
    binds: Option<Ident>,
    priority: LitInt,
    configurations: Vec<ConfigurationInput>,
    spawn_bindings: Vec<SpawnBindingInput>,
}

impl Parse for AppTaskInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        let body;
        braced!(body in input);
        let mut binds = None;
        let mut priority = None;
        let mut configurations = Vec::new();
        let mut spawn_bindings = Vec::new();

        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "binds" => binds = Some(body.parse()?),
                "priority" => priority = Some(body.parse()?),
                "config" => {
                    let values;
                    braced!(values in body);
                    configurations =
                        Punctuated::<ConfigurationInput, Token![,]>::parse_terminated(&values)?
                            .into_iter()
                            .collect();
                }
                "spawn" => {
                    let values;
                    braced!(values in body);
                    spawn_bindings =
                        Punctuated::<SpawnBindingInput, Token![,]>::parse_terminated(&values)?
                            .into_iter()
                            .collect();
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `binds`, `priority`, `config`, or `spawn`",
                    ));
                }
            }
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            name,
            binds,
            priority: priority.ok_or_else(|| body.error("task requires `priority`"))?,
            configurations,
            spawn_bindings,
        })
    }
}

struct ConfigurationInput {
    name: Ident,
    ty: Type,
    value: Expr,
}

impl Parse for ConfigurationInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![=]>()?;
        let value = input.parse()?;
        Ok(Self { name, ty, value })
    }
}

struct SpawnBindingInput {
    alias: Ident,
    target: Ident,
}

impl Parse for SpawnBindingInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let alias = input.parse()?;
        input.parse::<Token![=>]>()?;
        let target = input.parse()?;
        Ok(Self { alias, target })
    }
}

struct DependencyRegistryInput {
    entries: Vec<DependencyInput>,
}

impl Parse for DependencyRegistryInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(Self {
            entries: Punctuated::<DependencyInput, Token![,]>::parse_terminated(input)?
                .into_iter()
                .collect(),
        })
    }
}

struct DependencyInput {
    id: Ident,
    package: LitStr,
    version: LitStr,
    default_features: LitBool,
    features: Vec<LitStr>,
}

impl Parse for DependencyInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let id = input.parse()?;
        input.parse::<Token![=>]>()?;
        let body;
        braced!(body in input);
        let mut package = None;
        let mut version = None;
        let mut default_features = None;
        let mut features = None;

        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "package" => package = Some(body.parse()?),
                "version" => version = Some(body.parse()?),
                "default_features" => default_features = Some(body.parse()?),
                "features" => {
                    let values;
                    bracketed!(values in body);
                    features = Some(
                        Punctuated::<LitStr, Token![,]>::parse_terminated(&values)?
                            .into_iter()
                            .collect(),
                    );
                }
                _ => return Err(syn::Error::new(key.span(), "unknown dependency property")),
            }
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            id,
            package: package.ok_or_else(|| body.error("dependency requires `package`"))?,
            version: version.ok_or_else(|| body.error("dependency requires `version`"))?,
            default_features: default_features
                .unwrap_or_else(|| LitBool::new(true, proc_macro2::Span::call_site())),
            features: features.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_embedded_example_without_compiling_it() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../embedded");
        let application = load_application(&workspace).unwrap();

        assert_eq!(application.module, "app");
        assert_eq!(application.target.mcu, "STM32F401RET6");
        assert_eq!(application.target.hal, "stm32f4xx_hal");
        assert_eq!(application.dispatchers, ["USART1"]);
        assert_eq!(
            application.monotonic,
            Some(LoadedMonotonic {
                name: "Mono".to_owned(),
                source: LoadedMonotonicSource::Timer("TIM5".to_owned()),
                tick_hz: 1_000_000,
            })
        );
        assert!(application.shared_source.contains("enable_blink : bool"));
        assert!(
            application
                .local_source
                .contains("hello_timer : CounterHz < TIM2 >")
        );
        assert!(application.init_source.contains("GPIOA . split"));
        assert_eq!(application.tasks.len(), 2);

        let blink = &application.tasks[0];
        assert_eq!(blink.name, "blink");
        assert_eq!(blink.priority, 1);
        assert_eq!(blink.implementation.shared_resources, ["enable_blink"]);
        assert_eq!(blink.implementation.local_resources, ["led"]);
        assert_eq!(blink.implementation.config_keys, ["period_ms"]);
        assert!(blink.implementation.source.contains("Mono :: delay"));

        let timer = &application.tasks[1];
        assert_eq!(timer.name, "timer_interrupt");
        assert_eq!(timer.interrupt.as_deref(), Some("TIM2"));
        assert_eq!(timer.implementation.dependencies[0].id, "Defmt");

        assert_eq!(application.dependencies.len(), 2);
        assert_eq!(application.dependencies[0].package, "fugit");
        assert_eq!(application.dependencies[1].package, "defmt");
    }
}
