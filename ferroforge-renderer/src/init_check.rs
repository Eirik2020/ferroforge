//! Composition-generated, compile-only interfaces for native system init.

use std::{
    fs,
    path::{Path, PathBuf},
};

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, FnArg};

use crate::{
    RenderError,
    composition::{MonotonicProfile, ValidatedComposition},
    source::{InitPackage, InitSource},
};

pub const INITIAL_INIT_RUST_TARGET: &str = "thumbv7em-none-eabihf";

#[derive(Clone, Debug)]
pub struct InitCheckOptions<'a> {
    pub package_name: &'a str,
    /// Directory for `interfaces.rs` and `.cargo/config.toml`. Use the init
    /// package root when Rust Analyzer should discover the generated setup.
    pub output_dir: &'a Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedInitCheck {
    pub root: PathBuf,
    /// The original init package manifest checked with the generated config.
    pub manifest: PathBuf,
    /// The original authored library root; it is not copied into `root`.
    pub library_source: PathBuf,
    pub interface_source: PathBuf,
    pub cargo_config: PathBuf,
}

/// Render a no-std library which checks the complete native init body without
/// executing it or linking task implementations.
pub fn render_init_check(
    package: &InitPackage,
    composition: &ValidatedComposition<'_>,
    options: InitCheckOptions<'_>,
) -> Result<RenderedInitCheck, RenderError> {
    validate_package_name(options.package_name)?;
    let source = render_init_check_interface(package, composition)?;
    let cargo_dir = options.output_dir.join(".cargo");
    fs::create_dir_all(&cargo_dir)?;
    let interface_source = options.output_dir.join("interfaces.rs");
    let cargo_config = cargo_dir.join("config.toml");
    fs::write(&interface_source, source)?;
    fs::write(
        &cargo_config,
        render_cargo_config(&interface_source, options.package_name),
    )?;
    Ok(RenderedInitCheck {
        root: options.output_dir.to_path_buf(),
        manifest: package.package.manifest.clone(),
        library_source: package.package.library_root.clone(),
        interface_source,
        cargo_config,
    })
}

pub fn render_init_check_interface(
    package: &InitPackage,
    composition: &ValidatedComposition<'_>,
) -> Result<String, RenderError> {
    require_initial_profile(composition)?;
    require_initial_source_scope(&package.init)?;
    let spawn_modules = composition
        .tasks
        .iter()
        .map(render_spawn_module)
        .collect::<Result<Vec<_>, RenderError>>()?;
    let tokens = quote! {
        pub mod init {
            pub struct Context {
                pub core: ::cortex_m::Peripherals,
                pub device: ::stm32f4xx_hal::pac::Peripherals,
            }
        }

        pub struct Mono;

        impl Mono {
            pub fn start(_syst: ::cortex_m::peripheral::SYST, _core_hz: u32) {}
        }

        #(#spawn_modules)*
    };
    let file: syn::File = syn::parse2(tokens)?;
    Ok(prettyplease::unparse(&file))
}

fn require_initial_profile(composition: &ValidatedComposition<'_>) -> Result<(), RenderError> {
    if composition.monotonic.as_ref() != Some(&MonotonicProfile::initial_systick()) {
        return Err(invalid(
            "independent init checking requires the initial SysTick profile at 1000 Hz with u32 time values",
        ));
    }
    Ok(())
}

fn require_initial_source_scope(init: &InitSource) -> Result<(), RenderError> {
    if !init.function.attrs.iter().any(is_qualified_init) {
        return Err(invalid(
            "initial independent init checking requires the qualified `#[ferroforge::init]` marker",
        ));
    }
    if !init.module.path.is_empty() {
        return Err(invalid(
            "initial independent init checking requires the init declaration at the crate root",
        ));
    }
    Ok(())
}

fn render_spawn_module(
    task: &crate::composition::ValidatedTask<'_>,
) -> Result<TokenStream, RenderError> {
    let name: syn::Ident = syn::parse_str(&task.source.name)
        .map_err(|_| invalid(format!("invalid task instance name `{}`", task.source.name)))?;
    let inputs = task
        .source
        .definition
        .function
        .sig
        .inputs
        .iter()
        .skip(1)
        .map(|input| {
            let FnArg::Typed(input) = input else {
                return Err(invalid(format!(
                    "task instance `{}` has a receiver input",
                    task.source.name
                )));
            };
            Ok(input.clone())
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    let error = spawn_error_type(&inputs);
    Ok(quote! {
        pub mod #name {
            #[inline]
            #[allow(unused_variables)]
            pub fn spawn(#(#inputs),*) -> ::core::result::Result<(), #error> {
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn spawn_error_type(inputs: &[syn::PatType]) -> TokenStream {
    match inputs {
        [] => quote!(()),
        [input] => {
            let ty = &input.ty;
            quote!(#ty)
        }
        inputs => {
            let types = inputs.iter().map(|input| &input.ty);
            quote!((#(#types),*))
        }
    }
}

fn validate_package_name(package_name: &str) -> Result<(), RenderError> {
    if package_name.is_empty()
        || !package_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid(format!(
            "invalid init check package name `{package_name}`"
        )));
    }
    Ok(())
}

fn render_cargo_config(interface: &Path, package_name: &str) -> String {
    let interface = interface.to_string_lossy();
    let interface = interface
        .strip_prefix(r"\\?\")
        .unwrap_or(&interface)
        .replace('\\', "/");
    format!(
        "# Generated for {package_name}.\n[build]\ntarget = \"{INITIAL_INIT_RUST_TARGET}\"\n\n[env]\nFERROFORGE_INIT_INTERFACES = {{ value = \"{interface}\", force = true }}\n"
    )
}

fn is_qualified_init(attribute: &Attribute) -> bool {
    let path = attribute.path();
    path.segments.len() == 2
        && path.segments[0].ident == "ferroforge"
        && path.segments[1].ident == "init"
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}
