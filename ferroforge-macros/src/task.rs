//! Call-through expansion: a reusable task becomes an ordinary generic `async
//! fn` over real trait bounds, with a real context struct in its own crate.
//!
//! Nothing here is a mock. The firmware depends on the task crate normally and
//! its RTIC handler constructs this context from its own, so the body is
//! compiled once, in place, by the compiler that also checks it standalone.

use ferroforge_contracts::{Resource, TaskArguments, TaskContract, TaskKind, identifier_key};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, ItemFn, Result, spanned::Spanned, visit_mut::VisitMut};

/// Uppercase configuration names become const generic parameters, so a value
/// supplied by composition is a constant inside the body.
fn const_name(name: &syn::Ident) -> syn::Ident {
    format_ident!("{}", name.to_string().to_uppercase(), span = name.span())
}

fn resource_type(resource: &Resource, bounds: &[(String, syn::Ident)]) -> TokenStream2 {
    if let Some(ty) = &resource.ty {
        return quote!(#ty);
    }
    let key = identifier_key(&resource.name);
    let generic = bounds
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, generic)| generic);
    match generic {
        Some(generic) => quote!(#generic),
        None => quote!(()),
    }
}

/// Rewrite `CONFIG.FIELD` reads to `__FfConfig::FIELD`, an associated const on
/// the configuration type the firmware supplies. Associated consts stay
/// compile-time, and unlike const generics they are inferred from the context
/// the caller constructs - so the adapter needs no turbofish.
///
/// This is a local rewrite inside the task's own expansion, not a cross-crate
/// move.
struct ConfigReader;

impl VisitMut for ConfigReader {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        if let syn::Expr::Field(field) = expr
            && let syn::Expr::Path(base) = field.base.as_ref()
            && base.path.is_ident("CONFIG")
            && let syn::Member::Named(name) = &field.member
        {
            let name = name.clone();
            *expr = syn::parse_quote!(__FfConfig::#name);
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expr);
    }
}

pub fn expand(contract: TaskContract, mut function: ItemFn) -> Result<TokenStream2> {
    let arguments: TaskArguments = contract.arguments.clone();
    let task_name = function.sig.ident.clone();
    let visibility = function.vis.clone();

    if !arguments.dependencies.is_empty() {
        return Err(Error::new(
            function.sig.span(),
            "task dependencies belong in Cargo.toml",
        ));
    }

    // One generic per bounded resource, named after its position so the
    // authored resource name stays the field name.
    let bound_generics = arguments
        .bounds
        .iter()
        .enumerate()
        .map(|(index, bound)| {
            (
                identifier_key(&bound.name),
                format_ident!("__FfResource{index}", span = bound.name.span()),
            )
        })
        .collect::<Vec<_>>();

    let local_fields = arguments.local.iter().map(|resource| {
        let name = &resource.name;
        let ty = resource_type(resource, &bound_generics);
        quote!(pub #name: &'__ff mut #ty,)
    });

    // A shared resource is whatever RTIC hands over: a real proxy implementing
    // `Mutex`, so `cx.shared.x.lock(..)` in the body is the real operation.
    let shared_generics = arguments
        .shared
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            (
                resource.clone(),
                format_ident!("__FfShared{index}", span = resource.name.span()),
            )
        })
        .collect::<Vec<_>>();
    let shared_fields = shared_generics.iter().map(|(resource, generic)| {
        let name = &resource.name;
        quote!(pub #name: #generic,)
    });
    let shared_bounds = shared_generics.iter().map(|(resource, generic)| {
        let ty = resource_type(resource, &bound_generics);
        quote!(#generic: ::rtic::Mutex<T = #ty>)
    });

    // Each outgoing alias is a closure field plus a same-named method, so the
    // authored `cx.spawn.report(v)` keeps working against a real call.
    let spawn_generics = arguments
        .spawn
        .iter()
        .enumerate()
        .map(|(index, spawn)| {
            (
                spawn.clone(),
                format_ident!("__FfSpawn{index}", span = spawn.name.span()),
            )
        })
        .collect::<Vec<_>>();
    let spawn_fields = spawn_generics.iter().map(|(spawn, generic)| {
        let name = &spawn.name;
        quote!(pub #name: #generic,)
    });
    let mut spawn_bounds = Vec::new();
    let mut spawn_methods = Vec::new();
    for (spawn, generic) in &spawn_generics {
        let name = &spawn.name;
        let inputs = spawn
            .inputs
            .as_ref()
            .ok_or_else(|| Error::new(name.span(), "a spawn alias needs a signature"))?;
        let types = inputs
            .iter()
            .map(|parameter| &parameter.ty)
            .collect::<Vec<_>>();
        let names = inputs
            .iter()
            .map(|parameter| &parameter.name)
            .collect::<Vec<_>>();
        let error = match types.as_slice() {
            [] => quote!(()),
            [ty] => quote!(#ty),
            types => quote!((#(#types),*)),
        };
        spawn_bounds.push(quote!(#generic: Fn(#(#types),*) -> ::core::result::Result<(), #error>));
        spawn_methods.push(quote! {
            #[inline]
            pub fn #name(&self, #(#names: #types),*)
                -> ::core::result::Result<(), #error> {
                (self.#name)(#(#names),*)
            }
        });
    }

    // Configuration becomes a trait of associated consts. The firmware
    // implements it per instance; the value stays a compile-time constant.
    let config_consts = arguments
        .config
        .iter()
        .map(|config| {
            let name = const_name(&config.name);
            let ty = config
                .ty
                .as_ref()
                .ok_or_else(|| Error::new(config.name.span(), "configuration needs a type"))?;
            Ok(quote!(const #name: #ty;))
        })
        .collect::<Result<Vec<_>>>()?;
    ConfigReader.visit_block_mut(&mut function.block);

    // The monotonic keeps the name the author imported, so `Mono::delay(..)` in
    // the body resolves to this type parameter, which the firmware fills with
    // its real monotonic.
    let monotonic_param = arguments
        .monotonic
        .as_ref()
        .map(|path| {
            path.get_ident()
                .cloned()
                .ok_or_else(|| Error::new(path.span(), "the monotonic must be a plain name"))
        })
        .transpose()?;

    let bound_params = arguments
        .bounds
        .iter()
        .zip(&bound_generics)
        .map(|(bound, (_, generic))| {
            let traits = &bound.traits;
            quote!(#generic: #traits)
        });
    let bound_names = bound_generics
        .iter()
        .map(|(_, generic)| generic)
        .collect::<Vec<_>>();
    let shared_names = shared_generics
        .iter()
        .map(|(_, generic)| generic)
        .collect::<Vec<_>>();
    let spawn_names = spawn_generics
        .iter()
        .map(|(_, generic)| generic)
        .collect::<Vec<_>>();
    // Built once: `quote!` consumes an iterator, and each list is used several
    // times below. The lifetime only appears when a task has local resources,
    // because otherwise nothing borrows and Rust rejects it as unused.
    let has_local = !arguments.local.is_empty();
    let mut local_params = Vec::new();
    if has_local {
        local_params.push(quote!('__ff));
    }
    local_params.extend(bound_names.iter().map(|generic| quote!(#generic)));
    let local_list = quote!(#(#local_params),*);

    // Every parameter lives on the context, so the caller specifies nothing:
    // constructing the context infers all of them, including the configuration
    // type and the monotonic carried as `PhantomData`.
    let mut context_params = local_params.clone();
    context_params.extend(shared_names.iter().map(|generic| quote!(#generic)));
    context_params.extend(spawn_names.iter().map(|generic| quote!(#generic)));
    context_params.push(quote!(__FfConfig));
    // Every task carries a monotonic slot whether or not its body uses one, so
    // the caller can construct any context without knowing the definition's
    // internals. The type parameter takes the name the author imported, so
    // `Mono::delay(..)` in the body resolves to it.
    let monotonic_name = monotonic_param
        .clone()
        .unwrap_or_else(|| format_ident!("__FfMono"));
    context_params.push(quote!(#monotonic_name));
    let context_generics = quote!(#(#context_params),*);

    let shared_list = quote!(#(#shared_names),*);
    let spawn_list = quote!(#(#spawn_names),*);
    // Bounded only when the task declared a monotonic, because the bound is what
    // drags `rtic-monotonics` and `fugit` into the task crate's dependencies. The
    // slot itself stays unconditional - a caller must be able to construct any
    // context the same way - but a task that never reads time must not have to
    // depend on the crates that describe time.
    //
    // The agreed initial profile: SysTick at 1 kHz with u32 time values.
    let monotonic_bound = monotonic_param.as_ref().map(|_| {
        quote!(#monotonic_name: ::rtic_monotonics::Monotonic<
            Duration = ::fugit::Duration<u32, 1, 1000>
        >,)
    });
    let monotonic_field = quote!(pub monotonic: ::core::marker::PhantomData<#monotonic_name>,);

    // Strip the authored context parameter; the real, generic one is declared
    // below with the same name, so the body still says `cx`.
    function.sig.inputs = function.sig.inputs.into_iter().skip(1).collect();
    let body = function.block;
    let signature_inputs = &function.sig.inputs;
    let output = &function.sig.output;
    let asyncness = (contract.kind == TaskKind::Software).then(|| quote!(async));
    let context_parameter = contract.context.clone();

    Ok(quote! {
        #[allow(non_snake_case)]
        #visibility mod #task_name {
            use super::*;

            pub struct Local<#local_list> {
                #(#local_fields)*
            }

            pub struct Shared<#shared_list> {
                #(#shared_fields)*
            }

            pub struct Spawn<#spawn_list> {
                #(#spawn_fields)*
            }

            impl<#spawn_list> Spawn<#spawn_list>
            where #(#spawn_bounds),* {
                #(#spawn_methods)*
            }

            /// Configuration the firmware supplies per instance. Associated
            /// consts stay compile-time and are usable in const positions.
            pub trait Config {
                #(#config_consts)*
            }

            pub struct Context<#context_generics> {
                pub local: Local<#local_list>,
                pub shared: Shared<#shared_list>,
                pub spawn: Spawn<#spawn_list>,
                pub config: ::core::marker::PhantomData<__FfConfig>,
                #monotonic_field
            }
        }

        #[allow(non_snake_case)]
        #visibility #asyncness fn #task_name<#context_generics>(
            mut #context_parameter: #task_name::Context<#context_generics>,
            #signature_inputs
        ) #output
        where
            __FfConfig: #task_name::Config,
            #monotonic_bound
            #(#bound_params,)*
            #(#shared_bounds,)*
            #(#spawn_bounds,)*
        #body
    })
}
