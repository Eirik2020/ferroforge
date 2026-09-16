//! Call-through rendering: emit an RTIC handler per instance that constructs
//! the reusable task's own context and calls it.
//!
//! Nothing is transplanted. The generated app depends on each task crate
//! normally, so bodies are compiled in place and every binding is checked by
//! the compiler rather than validated by this renderer.

use std::{fs, path::Path};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::{
    RenderError,
    composition::{TaskKind, ValidatedComposition, ValidatedTask},
    dependencies::{DependencyRequirement, DependencySource},
    source::TaskPackage,
    standalone::{
        RenderedStandaloneProject, StandaloneTarget, render_cargo_config, render_embed_config,
        render_memory_layout, toml_string, validate_package_name, validate_target,
    },
    transplant::RticAppShell,
};

const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Windows canonicalization prefix; Cargo manifests must not carry it.
const VERBATIM_PREFIX: &str = r"\\?\";

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}

fn ident(name: &str, what: &str) -> Result<Ident, RenderError> {
    syn::parse_str(name).map_err(|_| invalid(format!("invalid {what} `{name}`")))
}

/// `crate_name::module::path::definition`, the real Rust path to the reusable
/// task. If it is wrong, the generated app fails to compile at that call - the
/// renderer does not need to resolve it.
fn definition_path(task: &ValidatedTask<'_>) -> Result<TokenStream, RenderError> {
    let package = task.source.package_key().ok_or_else(|| {
        invalid(format!(
            "task instance `{}` has no package identity; call-through rendering needs one",
            task.source.name
        ))
    })?;
    let krate = ident(package, "package name")?;
    let modules = task
        .source
        .definition
        .id
        .module
        .path
        .iter()
        .map(|segment| ident(segment, "module name"))
        .collect::<Result<Vec<_>, _>>()?;
    let name = ident(&task.source.definition.id.name, "task definition")?;
    Ok(quote!(#krate #(:: #modules)* :: #name))
}

fn render_handler(
    task: &ValidatedTask<'_>,
    monotonic: Option<&Ident>,
) -> Result<TokenStream, RenderError> {
    let instance = ident(&task.source.name, "task instance")?;
    let path = definition_path(task)?;
    let arguments = &task.source.definition.contract.arguments;

    // Resource claims name the firmware's resources; the context fields keep
    // the reusable names, so the mapping is visible on one line each.
    let mut local_claims = Vec::new();
    let mut local_fields = Vec::new();
    for binding in &task.local {
        let requirement = ident(&binding.requirement, "resource requirement")?;
        let resource = ident(&binding.resource, "system resource")?;
        local_claims.push(resource.clone());
        local_fields.push(quote!(#requirement: cx.local.#resource));
    }
    let mut shared_claims = Vec::new();
    let mut shared_fields = Vec::new();
    for binding in &task.shared {
        let requirement = ident(&binding.requirement, "resource requirement")?;
        let resource = ident(&binding.resource, "system resource")?;
        shared_claims.push(resource.clone());
        shared_fields.push(quote!(#requirement: cx.shared.#resource));
    }
    local_claims.sort_by_key(ToString::to_string);
    shared_claims.sort_by_key(ToString::to_string);

    // Each alias becomes a closure onto the bound instance's real spawn.
    let mut spawn_fields = Vec::new();
    for binding in &task.spawn {
        let alias = ident(&binding.alias, "spawn alias")?;
        let target = ident(&binding.target, "spawn target")?;
        let declaration = arguments
            .spawn
            .iter()
            .find(|spawn| spawn.name == alias)
            .ok_or_else(|| {
                invalid(format!(
                    "task instance `{}` has no spawn alias `{alias}`",
                    task.source.name
                ))
            })?;
        let parameters = declaration
            .inputs
            .as_ref()
            .ok_or_else(|| invalid(format!("spawn alias `{alias}` has no signature")))?
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        spawn_fields.push(quote!(#alias: |#(#parameters),*| #target::spawn(#(#parameters),*)));
    }

    // Turbofish: one `_` per inferred context parameter, then the firmware's
    // monotonic, then a const argument per configuration value.
    let inferred = arguments.bounds.len() + arguments.shared.len() + arguments.spawn.len();
    let placeholders = (0..inferred).map(|_| quote!(_));
    let monotonic_argument = arguments
        .monotonic
        .as_ref()
        .map(|_| {
            monotonic.cloned().ok_or_else(|| {
                invalid(format!(
                    "task instance `{}` needs a monotonic, but the app declares none",
                    task.source.name
                ))
            })
        })
        .transpose()?;
    let mut configuration = Vec::new();
    for binding in &task.configuration {
        let value: syn::Expr = syn::parse_str(&binding.value).map_err(|_| {
            invalid(format!(
                "configuration `{}` value `{}` is not a Rust expression",
                binding.name, binding.value
            ))
        })?;
        configuration.push(quote!({ #value }));
    }
    let turbofish = quote!(::<#(#placeholders,)* #monotonic_argument #(, #configuration)*>);

    let priority = syn::LitInt::new(
        &task.priority.to_string(),
        proc_macro2::Span::call_site(),
    );
    let binds = task
        .interrupt
        .as_deref()
        .map(|interrupt| {
            ident(interrupt, "interrupt").map(|interrupt| quote!(binds = #interrupt,))
        })
        .transpose()?;
    let local_attribute = (!local_claims.is_empty()).then(|| quote!(, local = [#(#local_claims),*]));
    let shared_attribute =
        (!shared_claims.is_empty()).then(|| quote!(, shared = [#(#shared_claims),*]));

    let inputs = task
        .source
        .definition
        .contract
        .inputs
        .iter()
        .map(|parameter| {
            let name = &parameter.name;
            let ty = &parameter.ty;
            quote!(, #name: #ty)
        })
        .collect::<Vec<_>>();
    let forwarded = task
        .source
        .definition
        .contract
        .inputs
        .iter()
        .map(|parameter| {
            let name = &parameter.name;
            quote!(, #name)
        })
        .collect::<Vec<_>>();

    let software = task.kind() == TaskKind::Software;
    let asyncness = software.then(|| quote!(async));
    let awaiting = software.then(|| quote!(.await));
    let output = task
        .source
        .definition
        .contract
        .diverges
        .then(|| quote!(-> !));

    Ok(quote! {
        #[task(#binds priority = #priority #local_attribute #shared_attribute)]
        #asyncness fn #instance(cx: #instance::Context #(#inputs)*) #output {
            #path #turbofish(
                #path::Context {
                    local: #path::Local { #(#local_fields),* },
                    shared: #path::Shared { #(#shared_fields),* },
                    spawn: #path::Spawn { #(#spawn_fields),* },
                }
                #(#forwarded)*
            ) #awaiting
        }
    })
}

/// Render the complete app: the firmware's own shell plus one adapter per
/// selected instance.
pub fn render_callthrough_app(
    composition: &ValidatedComposition<'_>,
    shell: &RticAppShell,
) -> Result<String, RenderError> {
    let monotonic = composition
        .monotonic
        .as_ref()
        .map(|_| format_ident!("Mono"));
    let handlers = composition
        .tasks
        .iter()
        .map(|task| render_handler(task, monotonic.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;

    let crate_imports = shell
        .crate_imports
        .iter()
        .map(|source| syn::parse_str::<syn::Item>(source).map_err(RenderError::from))
        .collect::<Result<Vec<_>, _>>()?;
    let device: syn::Path = syn::parse_str(&shell.device)?;
    let app_module = ident(&shell.app_module, "RTIC app module")?;
    let dispatchers = shell
        .dispatchers
        .iter()
        .map(|name| ident(name, "dispatcher"))
        .collect::<Result<Vec<_>, _>>()?;
    let shared: syn::ItemStruct = syn::parse_str(&shell.shared)?;
    let local: syn::ItemStruct = syn::parse_str(&shell.local)?;
    let init: syn::ItemFn = syn::parse_str(&shell.init)?;
    let monotonic_declaration = monotonic.as_ref().map(|name| {
        quote! {
            use rtic_monotonics::systick::prelude::*;
            systick_monotonic!(#name, 1000);
        }
    });
    let monotonic_import = monotonic.as_ref().map(|name| quote!(use super::#name;));

    let tokens = quote! {
        #![no_std]
        #![no_main]
        #(#crate_imports)*
        #monotonic_declaration

        #[rtic::app(device = #device, dispatchers = [#(#dispatchers),*])]
        mod #app_module {
            use super::*;
            #monotonic_import

            #[shared]
            #shared

            #[local]
            #local

            #[init]
            #init

            #(#handlers)*
        }
    };
    let file: syn::File = syn::parse2(tokens)?;
    Ok(prettyplease::unparse(&file))
}

#[derive(Clone, Debug)]
pub struct CallthroughProjectOptions<'a> {
    pub package_name: &'a str,
    pub output_dir: &'a Path,
    pub shell: &'a RticAppShell,
    pub target: &'a StandaloneTarget,
    /// Backend and runtime selections owned by the firmware.
    pub system_dependencies: &'a [DependencyRequirement],
    /// Task crates the generated firmware depends on. Their own requirements
    /// are resolved by Cargo, so nothing is merged here.
    pub task_packages: &'a [&'a TaskPackage],
}

/// Emit a complete firmware project whose task bodies stay in their own crates.
pub fn render_callthrough_project(
    composition: &ValidatedComposition<'_>,
    options: CallthroughProjectOptions<'_>,
) -> Result<RenderedStandaloneProject, RenderError> {
    validate_package_name(options.package_name)?;
    validate_target(options.target)?;

    let source = render_callthrough_app(composition, options.shell)?;
    let manifest = render_manifest(
        options.package_name,
        options.system_dependencies,
        options.task_packages,
    )?;

    let root = options.output_dir.to_path_buf();
    let source_dir = root.join("src");
    let cargo_dir = root.join(".cargo");
    fs::create_dir_all(&source_dir)?;
    fs::create_dir_all(&cargo_dir)?;

    let manifest_path = root.join("Cargo.toml");
    let main_source = source_dir.join("main.rs");
    let memory_layout = root.join("memory.x");
    let cargo_config = cargo_dir.join("config.toml");
    let embed_config = root.join("Embed.toml");
    fs::write(&manifest_path, manifest)?;
    fs::write(&main_source, source)?;
    fs::write(&memory_layout, render_memory_layout(options.target))?;
    fs::write(&cargo_config, render_cargo_config(options.target))?;
    fs::write(&embed_config, render_embed_config(options.target))?;
    fs::write(root.join(".gitignore"), "/target/\n")?;

    Ok(RenderedStandaloneProject {
        root,
        manifest: manifest_path,
        main_source,
        memory_layout,
        cargo_config,
        embed_config,
    })
}

fn render_manifest(
    package_name: &str,
    system_dependencies: &[DependencyRequirement],
    task_packages: &[&TaskPackage],
) -> Result<String, RenderError> {
    let mut manifest = format!(
        "[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\nbuild = false\n\n[[bin]]\nname = {}\npath = \"src/main.rs\"\ntest = false\nbench = false\n\n[dependencies]\n",
        toml_string(package_name),
        toml_string(package_name),
    );
    for dependency in system_dependencies {
        if dependency.source != DependencySource::Registry(CRATES_IO_SOURCE.to_owned()) {
            return Err(invalid(format!(
                "call-through manifests support crates.io system dependencies; `{}` uses {}",
                dependency.package, dependency.source
            )));
        }
        let features = dependency
            .features
            .iter()
            .map(|feature| toml_string(feature))
            .collect::<Vec<_>>()
            .join(", ");
        manifest.push_str(&format!(
            "{} = {{ version = {}, default-features = {}, features = [{}] }}\n",
            dependency.name,
            toml_string(&dependency.version_requirement),
            dependency.default_features,
            features,
        ));
    }
    // The reusable crates are ordinary dependencies now, not source inputs.
    for package in task_packages {
        let path = package
            .root
            .to_string_lossy()
            .trim_start_matches(VERBATIM_PREFIX)
            .replace(std::path::MAIN_SEPARATOR, "/");
        manifest.push_str(&format!(
            "{} = {{ path = {} }}\n",
            package.name,
            toml_string(&path),
        ));
    }
    manifest.push_str(
        "\n[profile.release]\ncodegen-units = 1\ndebug = 2\nlto = true\nopt-level = \"s\"\n\n[workspace]\n",
    );
    Ok(manifest)
}
