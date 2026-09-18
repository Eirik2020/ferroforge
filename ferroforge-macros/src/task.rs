//! Call-through expansion: a reusable task becomes an ordinary generic `async
//! fn` over real trait bounds, with a real context struct in its own crate.
//!
//! Nothing here is a mock. The firmware depends on the task crate normally and
//! its RTIC handler constructs this context from its own, so the body is
//! compiled once, in place, by the compiler that also checks it standalone.

use ferroforge_contracts::{Resource, TaskArguments, TaskContract, TaskKind, identifier_key};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, ItemFn, Result, spanned::Spanned, visit::Visit};

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

/// Finds `CONFIG.FIELD`, the spelling before configuration became `CONFIG::FIELD`.
/// Diagnosis only: nothing in the body is rewritten. The compiler's own message
/// for it, "expected value, found type parameter", does not say what to write.
/// Like any reader of the body, it cannot see inside a macro call, where the
/// compiler's message is all there is.
#[derive(Default)]
struct OldConfigSpelling {
    found: Option<(proc_macro2::Span, String)>,
}

impl<'a> Visit<'a> for OldConfigSpelling {
    fn visit_expr_field(&mut self, field: &'a syn::ExprField) {
        if self.found.is_none()
            && let syn::Expr::Path(base) = field.base.as_ref()
            && base.path.is_ident("CONFIG")
            && let syn::Member::Named(name) = &field.member
        {
            self.found = Some((field.span(), name.to_string()));
        }
        syn::visit::visit_expr_field(self, field);
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

    // One generic per bounded local resource, named after its position so the
    // authored resource name stays the field name. A bounded shared resource
    // gets none: its bound is stated on the lock, `Mutex<T: Trait>`, because a
    // generic for it would appear in no struct field and could not be inferred.
    let local_bounds = arguments
        .bounds
        .iter()
        .enumerate()
        .filter(|(_, bound)| {
            arguments
                .local
                .iter()
                .any(|resource| identifier_key(&resource.name) == identifier_key(&bound.name))
        })
        .collect::<Vec<_>>();
    let bound_generics = local_bounds
        .iter()
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
        let bound = arguments
            .bounds
            .iter()
            .find(|bound| identifier_key(&bound.name) == identifier_key(&resource.name));
        match bound {
            Some(bound) => {
                let traits = &bound.traits;
                quote!(#generic: ::rtic::Mutex<T: #traits>)
            }
            None => {
                let ty = resource_type(resource, &bound_generics);
                quote!(#generic: ::rtic::Mutex<T = #ty>)
            }
        }
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

    let mut old_spelling = OldConfigSpelling::default();
    old_spelling.visit_block(&function.block);
    if let Some((span, name)) = old_spelling.found {
        return Err(Error::new(
            span,
            format!("configuration is read as `CONFIG::{name}`, an associated constant"),
        ));
    }

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

    let bound_params =
        local_bounds
            .iter()
            .zip(&bound_generics)
            .map(|((_, bound), (_, generic))| {
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
    // Named `CONFIG`, so the body reads configuration as `CONFIG::PERIOD_MS`:
    // an associated const of this parameter, resolved by the compiler wherever
    // it is written - inside a macro call too - with nothing rewritten.
    context_params.push(quote!(CONFIG));
    // Every task carries a monotonic slot whether or not its body uses one, so
    // the caller can construct any context without knowing the definition's
    // internals. The type parameter takes the name the author imported, so
    // `Mono::delay(..)` in the body resolves to it.
    let monotonic_name = monotonic_param
        .clone()
        .unwrap_or_else(|| format_ident!("__FfMono"));
    context_params.push(quote!(#monotonic_name));
    let context_generics = quote!(#(#context_params),*);

    // A local with an initial value is the task's own state. The definition
    // holds the values, in one struct with one constant, so the firmware
    // declares a single RTIC task-local of that type whatever it contains and
    // never needs to know the fields. `__FfBound` is what the firmware does
    // bind, plus that struct; `into_local` splits both into the `Local` the
    // body reads, so `cx.local.name` means the same either way.
    let (owned, supplied): (Vec<_>, Vec<_>) = arguments
        .local
        .iter()
        .partition(|resource| resource.init.is_some());
    let owned_names = owned
        .iter()
        .map(|resource| &resource.name)
        .collect::<Vec<_>>();
    let owned_types = owned
        .iter()
        .map(|resource| resource_type(resource, &bound_generics))
        .collect::<Vec<_>>();
    let owned_inits = owned
        .iter()
        .map(|resource| &resource.init)
        .collect::<Vec<_>>();
    let supplied_names = supplied
        .iter()
        .map(|resource| &resource.name)
        .collect::<Vec<_>>();
    let supplied_types = supplied
        .iter()
        .map(|resource| resource_type(resource, &bound_generics))
        .collect::<Vec<_>>();
    let bound_list = quote!('__ffb, #(#bound_names),*);
    let local_from_bound = match has_local {
        true => quote!('__ffb, #(#bound_names),*),
        false => quote!(#(#bound_names),*),
    };

    let shared_list = quote!(#(#shared_names),*);
    let spawn_list = quote!(#(#spawn_names),*);
    // Bounded only when the task declared a monotonic, because the bound is what
    // drags `rtic-monotonics` and `fugit` into the task crate's dependencies. The
    // slot itself stays unconditional - a caller must be able to construct any
    // context the same way - but a task that never reads time must not have to
    // depend on the crates that describe time.
    //
    // The agreed initial profile: SysTick at 1 kHz with u32 time values. Both
    // associated types are fixed, because a bound that fixed only `Duration`
    // left `Mono::now()` opaque - a task could wait but not take a timestamp.
    // Both are what `systick_monotonic!(Mono, 1000)` produces.
    let monotonic_bound = monotonic_param.as_ref().map(|_| {
        quote!(#monotonic_name: ::rtic_monotonics::Monotonic<
            Instant = ::fugit::Instant<u32, 1, 1000>,
            Duration = ::fugit::Duration<u32, 1, 1000>,
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
        #[allow(non_snake_case, non_camel_case_types)]
        #visibility mod #task_name {
            use super::*;

            pub struct Local<#local_list> {
                #(#local_fields)*
            }

            #[doc(hidden)]
            pub struct __FfTaskLocal {
                #(pub #owned_names: #owned_types,)*
            }

            impl __FfTaskLocal {
                /// Every initial value, as RTIC evaluates a task-local's:
                /// once, as a constant.
                pub const INIT: Self = Self {
                    #(#owned_names: #owned_inits,)*
                };
            }

            #[doc(hidden)]
            pub struct __FfBound<#bound_list> {
                pub __ff_task: &'__ffb mut __FfTaskLocal,
                #(pub #supplied_names: &'__ffb mut #supplied_types,)*
            }

            impl<#bound_list> __FfBound<#bound_list> {
                #[inline]
                pub fn into_local(self) -> Local<#local_from_bound> {
                    let __FfTaskLocal { #(#owned_names),* } = self.__ff_task;
                    Local {
                        #(#owned_names,)*
                        #(#supplied_names: self.#supplied_names,)*
                    }
                }
            }

            pub struct Shared<#shared_list> {
                #(#shared_fields)*
            }

            // Bounded on the struct as well as the function, so a firmware
            // binding an alias to a task with other inputs is told so at the
            // field it wrote, where the struct is built - not at the call.
            pub struct Spawn<#spawn_list>
            where #(#spawn_bounds),* {
                #(#spawn_fields)*
            }

            impl<#spawn_list> Spawn<#spawn_list>
            where #(#spawn_bounds),* {
                #(#spawn_methods)*
            }

            /// Configuration the firmware supplies per instance. Associated
            /// consts stay compile-time, though as a generic parameter's they
            /// cannot size an array on stable Rust.
            pub trait Config {
                #(#config_consts)*
            }

            pub struct Context<#context_generics>
            where #(#spawn_bounds),* {
                pub local: Local<#local_list>,
                pub shared: Shared<#shared_list>,
                pub spawn: Spawn<#spawn_list>,
                pub config: ::core::marker::PhantomData<CONFIG>,
                #monotonic_field
            }
        }

        #[allow(non_snake_case, non_camel_case_types)]
        #visibility #asyncness fn #task_name<#context_generics>(
            mut #context_parameter: #task_name::Context<#context_generics>,
            #signature_inputs
        ) #output
        where
            CONFIG: #task_name::Config,
            #monotonic_bound
            #(#bound_params,)*
            #(#shared_bounds,)*
            #(#spawn_bounds,)*
        #body
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferroforge_contracts::TaskArguments;

    fn expanded(arguments: &str, function: &str) -> Result<String> {
        let arguments = syn::parse_str::<TaskArguments>(arguments)?;
        let function = syn::parse_str::<ItemFn>(function)?;
        let contract = TaskContract::new(arguments, &function.sig)?;
        expand(contract, function).map(|output| output.to_string())
    }

    /// Configuration is the `CONFIG` type parameter's associated constant, so
    /// the body is passed through as written.
    #[test]
    fn configuration_is_read_through_the_config_parameter() {
        let output = expanded(
            "config = [period_ms: u32]",
            "async fn run(cx: run::Context) { let _ = CONFIG::PERIOD_MS; }",
        )
        .unwrap();
        assert!(output.contains("CONFIG :: PERIOD_MS"), "{output}");
        assert!(output.contains("CONFIG : run :: Config"), "{output}");
    }

    /// The spelling before it was `CONFIG::FIELD` is named with its fix,
    /// because the compiler's own message does not say what to write.
    #[test]
    fn the_old_config_spelling_says_what_to_write() {
        let error = expanded(
            "config = [period_ms: u32]",
            "async fn run(cx: run::Context) { let _ = CONFIG.PERIOD_MS; }",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("`CONFIG::PERIOD_MS`"), "{error}");
    }
}
