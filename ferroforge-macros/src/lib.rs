mod compose;
mod reusable;

use std::collections::{BTreeSet, HashSet};

use ferroforge_contracts::{
    InitContract, LegacyTaskArguments as TaskArgs, TaskArguments, TaskContract, identifier_key,
};
use heck::{ToShoutySnakeCase, ToUpperCamelCase};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use syn::{
    Attribute, Error, Expr, Fields, FnArg, GenericParam, Ident, ItemFn, ItemStruct, LitBool,
    LitInt, LitStr, Meta, Path, Result, Token, Type, TypeParam, braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::Visit,
};

fn variant_name(ident: &Ident) -> Ident {
    let name = ident.to_string().to_upper_camel_case();
    format_ident!("{}", name, span = ident.span())
}

fn const_name(ident: &Ident) -> Ident {
    let name = ident.to_string().to_shouty_snake_case();
    format_ident!("{}", name, span = ident.span())
}

fn remove_marker_attribute(
    attributes: &mut Vec<Attribute>,
    marker: &str,
    span: Span,
) -> Result<()> {
    let matching: Vec<_> = attributes
        .iter()
        .enumerate()
        .filter(|(_, attribute)| attribute.path().is_ident(marker))
        .map(|(index, _)| index)
        .collect();

    if matching.len() != 1 {
        return Err(Error::new(
            span,
            format!("expected exactly one `#[{marker}]` attribute"),
        ));
    }

    let attribute = attributes.remove(matching[0]);
    if !matches!(attribute.meta, Meta::Path(_)) {
        return Err(Error::new(
            attribute.path().span(),
            format!("`#[{marker}]` does not accept arguments"),
        ));
    }

    Ok(())
}

struct DependencyRegistryEntry {
    id: Ident,
    package: LitStr,
    version: LitStr,
    default_features: LitBool,
    features: Vec<LitStr>,
}

impl Parse for DependencyRegistryEntry {
    fn parse(input: ParseStream) -> Result<Self> {
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
                "package" => {
                    if package.is_some() {
                        return Err(Error::new(key.span(), "package is declared more than once"));
                    }
                    package = Some(body.parse()?);
                }
                "version" => {
                    if version.is_some() {
                        return Err(Error::new(key.span(), "version is declared more than once"));
                    }
                    version = Some(body.parse()?);
                }
                "default_features" => {
                    if default_features.is_some() {
                        return Err(Error::new(
                            key.span(),
                            "default_features is declared more than once",
                        ));
                    }
                    default_features = Some(body.parse()?);
                }
                "features" => {
                    if features.is_some() {
                        return Err(Error::new(
                            key.span(),
                            "features are declared more than once",
                        ));
                    }
                    let feature_body;
                    bracketed!(feature_body in body);
                    features = Some(
                        Punctuated::<LitStr, Token![,]>::parse_terminated(&feature_body)?
                            .into_iter()
                            .collect(),
                    );
                }
                _ => {
                    return Err(Error::new(
                        key.span(),
                        "expected `package`, `version`, `default_features`, or `features`",
                    ));
                }
            }

            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            } else if !body.is_empty() {
                return Err(body.error("expected `,`"));
            }
        }

        Ok(Self {
            id,
            package: package.ok_or_else(|| {
                Error::new(input.span(), "dependency declaration requires `package`")
            })?,
            version: version.ok_or_else(|| {
                Error::new(input.span(), "dependency declaration requires `version`")
            })?,
            default_features: default_features
                .unwrap_or_else(|| LitBool::new(true, Span::call_site())),
            features: features.unwrap_or_default(),
        })
    }
}

struct DependencyRegistryInput {
    entries: Vec<DependencyRegistryEntry>,
}

impl Parse for DependencyRegistryInput {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            entries: Punctuated::<DependencyRegistryEntry, Token![,]>::parse_terminated(input)?
                .into_iter()
                .collect(),
        })
    }
}

#[proc_macro]
pub fn dependency_registry(input: TokenStream) -> TokenStream {
    let registry = parse_macro_input!(input as DependencyRegistryInput);

    match expand_dependency_registry(registry) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_dependency_registry(registry: DependencyRegistryInput) -> Result<TokenStream2> {
    let mut ids = HashSet::new();
    let mut packages = HashSet::new();

    for entry in &registry.entries {
        if !ids.insert(entry.id.to_string()) {
            return Err(Error::new(
                entry.id.span(),
                "dependency ID is declared more than once",
            ));
        }

        if !packages.insert(entry.package.value()) {
            return Err(Error::new(
                entry.package.span(),
                "dependency package is declared more than once",
            ));
        }

        let mut features = HashSet::new();
        for feature in &entry.features {
            if !features.insert(feature.value()) {
                return Err(Error::new(
                    feature.span(),
                    "dependency feature is declared more than once",
                ));
            }
        }
    }

    let ids = registry.entries.iter().map(|entry| &entry.id);
    let definitions = registry.entries.iter().map(|entry| {
        let id = &entry.id;
        let package = &entry.package;
        let version = &entry.version;
        let default_features = &entry.default_features;
        let features = &entry.features;

        quote! {
            ::ferroforge::DependencyDefinition {
                id: DependencyId::#id,
                package: #package,
                version: #version,
                default_features: #default_features,
                features: &[#(#features),*],
            }
        }
    });
    let resolution_arms = registry.entries.iter().map(|entry| {
        let id = &entry.id;
        let package = &entry.package;
        let version = &entry.version;
        let default_features = &entry.default_features;
        let features = &entry.features;

        quote! {
            Self::#id => ::ferroforge::DependencyDefinition {
                id: Self::#id,
                package: #package,
                version: #version,
                default_features: #default_features,
                features: &[#(#features),*],
            }
        }
    });
    let catalog_entries = registry.entries.iter().map(|entry| {
        let id = &entry.id;
        let package = &entry.package;
        let version = &entry.version;
        let default_features = &entry.default_features;
        let features = &entry.features;

        quote! {
            ::ferroforge::DependencyCatalogEntry {
                id: stringify!(#id),
                package: #package,
                version: #version,
                default_features: #default_features,
                features: &[#(#features),*],
            }
        }
    });

    Ok(quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) enum DependencyId {
            #(#ids),*
        }

        impl DependencyId {
            pub(crate) const fn resolve(self) -> ::ferroforge::DependencyDefinition<Self> {
                match self {
                    #(#resolution_arms),*
                }
            }
        }

        pub(crate) const DEPENDENCY_REGISTRY:
            &[::ferroforge::DependencyDefinition<DependencyId>] = &[
                #(#definitions),*
            ];

        pub(crate) const DEPENDENCY_CATALOG:
            &[::ferroforge::DependencyCatalogEntry] = &[
                #(#catalog_entries),*
            ];
    })
}

/// The firmware's authored composition, expanded in place into a real
/// `#[rtic::app]`. Init and resources are written here and never move; each
/// task declaration becomes an adapter that calls the reusable definition.
#[proc_macro]
pub fn compose(input: TokenStream) -> TokenStream {
    let composition = parse_macro_input!(input as compose::Composition);
    match compose::expand(composition) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Call-through reusable task: expands to a real generic context and an
/// ordinary generic function, with no mock layer. The firmware depends on this
/// crate and its RTIC handler calls the function, so nothing is transplanted.
///
/// Runs alongside `task` during migration; `task` keeps the mock expansion.
#[proc_macro_attribute]
pub fn reusable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as TaskArguments);
    let function = parse_macro_input!(item as ItemFn);

    let output = TaskContract::new(args, &function.sig)
        .and_then(|contract| reusable::expand(contract, function));

    match output {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as TaskArguments);
    let function = parse_macro_input!(item as ItemFn);

    let output = if args.uses_standalone_contract() {
        TaskContract::new(args, &function.sig)
            .and_then(|contract| expand_standalone_task(contract, function))
    } else {
        args.into_legacy()
            .and_then(|legacy| expand_task(legacy, function))
    };

    match output {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Marks a standalone system initialization function for source discovery.
///
/// Composition-generated checking and firmware source provide the surrounding
/// `init::Context`, resource types, spawn entry points, and monotonic. The
/// attribute validates the bounded initial signature and parses the generated
/// checking interface named by `FERROFORGE_INIT_INTERFACES` directly into its
/// expansion, preserving the authored function and its source spans.
#[proc_macro_attribute]
pub fn init(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(Span::call_site(), "`#[init]` does not accept arguments")
            .to_compile_error()
            .into();
    }
    let function = parse_macro_input!(item as ItemFn);
    match InitContract::new(&function.sig) {
        Ok(_) => match load_init_interfaces() {
            Ok(interfaces) => quote! {
                #interfaces
                #function
            }
            .into(),
            Err(error) => error.to_compile_error().into(),
        },
        Err(error) => error.to_compile_error().into(),
    }
}

fn load_init_interfaces() -> Result<TokenStream2> {
    let path = std::env::var_os("FERROFORGE_INIT_INTERFACES").ok_or_else(|| {
        Error::new(
            Span::call_site(),
            "`FERROFORGE_INIT_INTERFACES` must name the generated init checking interface",
        )
    })?;
    let source = std::fs::read_to_string(&path).map_err(|error| {
        Error::new(
            Span::call_site(),
            format!(
                "failed to read generated init checking interface `{}`: {error}",
                std::path::Path::new(&path).display()
            ),
        )
    })?;
    source.parse().map_err(|error| {
        Error::new(
            Span::call_site(),
            format!(
                "failed to parse generated init checking interface `{}`: {error}",
                std::path::Path::new(&path).display()
            ),
        )
    })
}

/// Marks an item as part of the embedded firmware side of a declaration.
#[proc_macro_attribute]
pub fn firmware(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(Span::call_site(), "`#[firmware]` does not accept arguments")
            .to_compile_error()
            .into();
    }

    let item = TokenStream2::from(item);

    quote! {
        #[cfg(target_arch = "arm")]
        #item
    }
    .into()
}

fn expand_task(args: TaskArgs, function: ItemFn) -> Result<TokenStream2> {
    let task_name = &function.sig.ident;
    let visibility = &function.vis;

    let spawn_count = args.spawns.len();
    let config_count = args.configs.len();
    let dependency_count = args.dependencies.len();
    let argument_count = function.sig.inputs.len();
    let is_async = function.sig.asyncness.is_some();
    let config_variants = args.configs.iter().map(variant_name);
    let shared_count = args.shared.len();
    let shared_marker_types = args.shared.iter().map(variant_name);
    let shared_key_variants = args.shared.iter().map(variant_name);
    let shared_check_variants = args.shared.iter().map(variant_name);
    let shared_fields = args.shared.iter().map(|shared| {
        let marker = variant_name(shared);

        quote! {
            pub #shared: ::ferroforge::MockShared<shared_resources::#marker>,
        }
    });
    let shared_bind_arms = args.shared.iter().map(|shared| {
        let marker = variant_name(shared);

        quote! {
            ($task:ident, #shared, $resource_type:ty) => {
                impl ::ferroforge::SharedResourceSpec for $task::shared_resources::#marker {
                    type Value = $resource_type;
                }
            };
        }
    });
    let local_count = args.local.len();
    let local_marker_types = args.local.iter().map(variant_name);
    let local_key_variants = args.local.iter().map(variant_name);
    let local_check_variants = args.local.iter().map(variant_name);
    let local_fields = args.local.iter().map(|local| {
        let marker = variant_name(local);

        quote! {
            pub #local: ::ferroforge::MockLocal<local_resources::#marker>,
        }
    });
    let arm_local_fields = args.local.iter().map(|local| {
        let marker = variant_name(local);

        quote! {
            pub #local: &'static mut
                <local_resources::#marker as ::ferroforge::LocalResourceSpec>::Value,
        }
    });
    let local_bind_arms = args.local.iter().map(|local| {
        let marker = variant_name(local);

        quote! {
            ($task:ident, #local, $resource_type:ty) => {
                impl ::ferroforge::LocalResourceSpec for $task::local_resources::#marker {
                    type Value = $resource_type;
                }
            };
        }
    });
    let spawn_variants = args.spawns.iter().map(variant_name);
    let task_parameters: Vec<_> = function.sig.inputs.iter().skip(1).collect();
    let dependency_checks: Vec<_> = args
        .dependencies
        .iter()
        .map(|dependency| {
            let id = &dependency.id;

            quote! {
                const _: $dependency_id = $dependency_id::#id;
            }
        })
        .collect();
    let dependency_requirements: Vec<_> = args
        .dependencies
        .iter()
        .map(|dependency| {
            let id = &dependency.id;
            let features = &dependency.features;

            quote! {
                ::ferroforge::DependencyRequirement {
                    dependency: $dependency_id::#id,
                    features: &[#(#features),*],
                }
            }
        })
        .collect();
    let renderer_dependencies: Vec<_> = args
        .dependencies
        .iter()
        .map(|dependency| {
            let id = &dependency.id;
            let features = &dependency.features;

            quote! {
                ::ferroforge::TaskDependencyDefinition {
                    id: stringify!(#id),
                    features: &[#(#features),*],
                }
            }
        })
        .collect();
    let renderer_shared_resources = &args.shared;
    let renderer_local_resources = &args.local;
    let renderer_config_keys = &args.configs;
    let renderer_spawn_aliases = &args.spawns;
    let missing_registry_check = if args.dependencies.is_empty() {
        quote! {}
    } else {
        quote! {
            compile_error!(concat!(
                "task `",
                stringify!(#task_name),
                "` declares dependencies, but the app has no `dependency_registry`",
            ));
        }
    };

    Ok(quote! {
        #function

        #visibility mod #task_name {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum Spawn {
                #(
                    #spawn_variants,
                )*
            }

            #[derive(Debug, Clone, Copy, Default)]
            pub struct SpawnHandle;

            #[inline]
            #[allow(unused_variables)]
            pub fn spawn(
                #(
                    #task_parameters,
                )*
            ) -> ::core::result::Result<(), ::ferroforge::SpawnError> {
                ::core::result::Result::Ok(())
            }

            macro_rules! __ferroforge_define_spawn {
                ($handle:path, $alias:ident) => {
                    impl $handle {
                        #[inline]
                        #[allow(unused_variables)]
                        pub fn $alias(
                            &self,
                            #(
                                #task_parameters,
                            )*
                        ) -> ::core::result::Result<(), ::ferroforge::SpawnError> {
                            ::core::result::Result::Ok(())
                        }
                    }
                };
            }

            pub(crate) use __ferroforge_define_spawn;

            pub mod shared_resources {
                #(
                    pub struct #shared_marker_types;
                )*
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum SharedKey {
                #(
                    #shared_key_variants,
                )*
            }

            macro_rules! __ferroforge_check_shared {
                ($resources:ident) => {
                    #(
                        const _: $resources = $resources::#shared_check_variants;
                    )*
                };
            }

            pub(crate) use __ferroforge_check_shared;

            macro_rules! __ferroforge_bind_shared {
                #(
                    #shared_bind_arms
                )*

                ($_task:ident, $_resource:ident, $_resource_type:ty) => {};
            }

            pub(crate) use __ferroforge_bind_shared;

            pub mod local_resources {
                #(
                    pub struct #local_marker_types;
                )*
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum LocalKey {
                #(
                    #local_key_variants,
                )*
            }

            macro_rules! __ferroforge_check_local {
                ($resources:ident) => {
                    #(
                        const _: $resources = $resources::#local_check_variants;
                    )*
                };
            }

            pub(crate) use __ferroforge_check_local;

            macro_rules! __ferroforge_bind_local {
                #(
                    #local_bind_arms
                )*

                ($_task:ident, $_resource:ident, $_resource_type:ty) => {};
            }

            pub(crate) use __ferroforge_bind_local;

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum ConfigKey {
                #(
                    #config_variants,
                )*
            }

            #[derive(Debug, Clone, Copy, Default)]
            pub struct Config;

            pub const IMPLEMENTATION: ::ferroforge::TaskImplementationDefinition =
                ::ferroforge::TaskImplementationDefinition {
                    source: stringify!(#function),
                    shared_resources: &[
                        #(stringify!(#renderer_shared_resources)),*
                    ],
                    local_resources: &[
                        #(stringify!(#renderer_local_resources)),*
                    ],
                    config_keys: &[
                        #(stringify!(#renderer_config_keys)),*
                    ],
                    spawn_aliases: &[
                        #(stringify!(#renderer_spawn_aliases)),*
                    ],
                    dependencies: &[#(#renderer_dependencies),*],
                };

            macro_rules! __ferroforge_check_dependencies {
                ($dependency_id:ident) => {
                    #(#dependency_checks)*
                };
            }

            pub(crate) use __ferroforge_check_dependencies;

            macro_rules! __ferroforge_dependency_requirements {
                ($dependency_id:ident) => {
                    &[
                        #(#dependency_requirements),*
                    ]
                };
            }

            pub(crate) use __ferroforge_dependency_requirements;

            macro_rules! __ferroforge_require_no_dependencies {
                () => {
                    #missing_registry_check
                };
            }

            pub(crate) use __ferroforge_require_no_dependencies;

            #[derive(Debug, Clone, Copy, Default)]
            pub struct Shared {
                #(
                    #shared_fields
                )*
            }

            #[cfg(not(target_arch = "arm"))]
            #[derive(Debug, Clone, Copy, Default)]
            pub struct Local {
                #(
                    #local_fields
                )*
            }

            #[cfg(target_arch = "arm")]
            pub struct Local {
                #(
                    #arm_local_fields
                )*
            }

            #[cfg(not(target_arch = "arm"))]
            #[derive(Debug, Clone, Copy, Default)]
            pub struct Context {
                pub local: Local,
                pub shared: Shared,
                pub spawn: SpawnHandle,
            }

            #[cfg(target_arch = "arm")]
            pub struct Context {
                pub local: Local,
                pub shared: Shared,
                pub spawn: SpawnHandle,
            }

            pub const SPAWN_COUNT: usize = #spawn_count;
            pub const CONFIG_COUNT: usize = #config_count;
            pub const DEPENDENCY_COUNT: usize = #dependency_count;
            pub const LOCAL_COUNT: usize = #local_count;
            pub const SHARED_COUNT: usize = #shared_count;
            pub const ARGUMENT_COUNT: usize = #argument_count;
            pub const IS_ASYNC: bool = #is_async;
        }
    })
}

fn expand_standalone_task(contract: TaskContract, mut function: ItemFn) -> Result<TokenStream2> {
    let task_name = function.sig.ident.clone();
    let visibility = function.vis.clone();
    let arguments = contract.arguments;
    let has_resources = !arguments.local.is_empty() || !arguments.shared.is_empty();

    let mut generic_resources = std::collections::BTreeMap::new();
    for (index, bound) in arguments.bounds.iter().enumerate() {
        generic_resources.insert(
            identifier_key(&bound.name),
            (
                format_ident!("__FerroforgeResource{index}", span = bound.name.span()),
                bound.traits.clone(),
            ),
        );
    }

    if has_resources {
        function
            .sig
            .generics
            .params
            .push(GenericParam::Lifetime(syn::parse_quote!('__ferroforge)));
    }
    for (generic, bounds) in generic_resources.values() {
        let parameter: TypeParam = syn::parse_quote!(#generic: #bounds);
        function
            .sig
            .generics
            .params
            .push(GenericParam::Type(parameter));
    }

    let context_generics = generic_resources
        .values()
        .map(|(generic, _)| generic)
        .collect::<Vec<_>>();
    let context_type = if has_resources {
        syn::parse2(quote!(#task_name::Context<'__ferroforge #(, #context_generics)*>))?
    } else {
        syn::parse2(quote!(#task_name::Context))?
    };
    let Some(FnArg::Typed(context)) = function.sig.inputs.first_mut() else {
        return Err(Error::new(
            function.sig.span(),
            "standalone task needs a typed context parameter",
        ));
    };
    *context.ty = context_type;

    let local_generics = resource_generics(&arguments.local, &generic_resources);
    let shared_generics = resource_generics(&arguments.shared, &generic_resources);
    let local_declaration = generic_declaration(!arguments.local.is_empty(), &local_generics);
    let local_use = generic_use(!arguments.local.is_empty(), &local_generics);
    let shared_declaration = generic_declaration(!arguments.shared.is_empty(), &shared_generics);
    let shared_use = generic_use(!arguments.shared.is_empty(), &shared_generics);
    let context_declaration = generic_declaration(has_resources, &generic_resources);

    let local_fields = arguments
        .local
        .iter()
        .map(|resource| {
            let name = &resource.name;
            let ty = resource_type(resource, &generic_resources);
            quote!(pub #name: &'__ferroforge mut #ty,)
        })
        .collect::<Vec<_>>();
    let shared_fields = arguments
        .shared
        .iter()
        .map(|resource| {
            let name = &resource.name;
            let ty = resource_type(resource, &generic_resources);
            quote!(pub #name: ::ferroforge::MockSharedRef<'__ferroforge, #ty>,)
        })
        .collect::<Vec<_>>();

    let spawn_methods = arguments
        .spawn
        .iter()
        .map(|spawn| {
            let name = &spawn.name;
            let inputs = spawn
                .inputs
                .as_ref()
                .expect("standalone spawn signatures were validated");
            let parameters = inputs.iter().map(|parameter| {
                let name = &parameter.name;
                let ty = &parameter.ty;
                quote!(#name: #ty)
            });
            let error = spawn_error_type(inputs);
            quote! {
                #[inline]
                #[allow(unused_variables)]
                pub fn #name(
                    &self,
                    #(#parameters),*
                ) -> ::core::result::Result<(), #error> {
                    ::core::result::Result::Ok(())
                }
            }
        })
        .collect::<Vec<_>>();

    let mut config_fields = Vec::new();
    let mut config_values = Vec::new();
    if !arguments.config.is_empty() {
        reject_config_binding(&function)?;
        for config in &arguments.config {
            let name = const_name(&config.name);
            let ty = config
                .ty
                .as_ref()
                .expect("standalone configuration types were validated");
            let value = standalone_config_default(ty)?;
            config_fields.push(quote!(pub #name: #ty,));
            config_values.push(quote!(#name: #value,));
        }
        let original = function.block;
        function.block = Box::new(syn::parse2(quote!({
            const CONFIG: #task_name::Config = #task_name::__FERROFORGE_CONFIG;
            #original
        }))?);
    }

    Ok(quote! {
        #visibility mod #task_name {
            #[allow(unused_imports)]
            use super::*;

            pub struct Local #local_declaration {
                #(#local_fields)*
            }

            pub struct Shared #shared_declaration {
                #(#shared_fields)*
            }

            #[derive(Debug, Clone, Copy, Default)]
            pub struct SpawnHandle;

            impl SpawnHandle {
                #(#spawn_methods)*
            }

            #[allow(non_snake_case)]
            pub(super) struct Config {
                #(#config_fields)*
            }

            #[allow(non_snake_case)]
            pub(super) const __FERROFORGE_CONFIG: Config = Config {
                #(#config_values)*
            };

            pub struct Context #context_declaration {
                pub local: Local #local_use,
                pub shared: Shared #shared_use,
                pub spawn: SpawnHandle,
            }
        }

        #function
    })
}

fn resource_generics(
    resources: &[ferroforge_contracts::Resource],
    generics: &std::collections::BTreeMap<
        String,
        (Ident, Punctuated<syn::TypeParamBound, Token![+]>),
    >,
) -> std::collections::BTreeMap<String, (Ident, Punctuated<syn::TypeParamBound, Token![+]>)> {
    resources
        .iter()
        .filter_map(|resource| {
            let key = identifier_key(&resource.name);
            generics.get(&key).cloned().map(|generic| (key, generic))
        })
        .collect()
}

fn generic_declaration(
    has_resources: bool,
    generics: &std::collections::BTreeMap<
        String,
        (Ident, Punctuated<syn::TypeParamBound, Token![+]>),
    >,
) -> TokenStream2 {
    if !has_resources {
        return quote!();
    }
    let parameters = generics
        .values()
        .map(|(generic, bounds)| quote!(#generic: #bounds));
    quote!(<'__ferroforge #(, #parameters)*>)
}

fn generic_use(
    has_resources: bool,
    generics: &std::collections::BTreeMap<
        String,
        (Ident, Punctuated<syn::TypeParamBound, Token![+]>),
    >,
) -> TokenStream2 {
    if !has_resources {
        return quote!();
    }
    let parameters = generics.values().map(|(generic, _)| generic);
    quote!(<'__ferroforge #(, #parameters)*>)
}

fn resource_type(
    resource: &ferroforge_contracts::Resource,
    generics: &std::collections::BTreeMap<
        String,
        (Ident, Punctuated<syn::TypeParamBound, Token![+]>),
    >,
) -> TokenStream2 {
    if let Some(ty) = &resource.ty {
        quote!(#ty)
    } else {
        let (generic, _) = &generics[&identifier_key(&resource.name)];
        quote!(#generic)
    }
}

fn spawn_error_type(inputs: &[ferroforge_contracts::Parameter]) -> TokenStream2 {
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

fn standalone_config_default(ty: &Type) -> Result<TokenStream2> {
    if let Type::Path(path) = ty
        && path.qself.is_none()
        && path.path.is_ident("u32")
    {
        return Ok(quote!(0));
    }
    Err(Error::new(
        ty.span(),
        "initial standalone configuration checking supports only `u32`",
    ))
}

fn reject_config_binding(function: &ItemFn) -> Result<()> {
    struct FindConfig {
        span: Option<Span>,
    }

    impl<'ast> Visit<'ast> for FindConfig {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            if pattern.ident == "CONFIG" {
                self.span.get_or_insert(pattern.ident.span());
            }
            syn::visit::visit_pat_ident(self, pattern);
        }

        fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
            if item.ident == "CONFIG" {
                self.span.get_or_insert(item.ident.span());
            }
            syn::visit::visit_item_const(self, item);
        }

        fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
            if item.ident == "CONFIG" {
                self.span.get_or_insert(item.ident.span());
            }
            syn::visit::visit_item_static(self, item);
        }
    }

    let mut visitor = FindConfig { span: None };
    visitor.visit_block(&function.block);
    if let Some(span) = visitor.span {
        return Err(Error::new(
            span,
            "`CONFIG` is reserved for standalone task configuration",
        ));
    }
    if let Some(parameter) = function.sig.inputs.iter().find_map(|argument| {
        let FnArg::Typed(argument) = argument else {
            return None;
        };
        let syn::Pat::Ident(pattern) = argument.pat.as_ref() else {
            return None;
        };
        (pattern.ident == "CONFIG").then_some(pattern.ident.span())
    }) {
        return Err(Error::new(
            parameter,
            "`CONFIG` is reserved for standalone task configuration",
        ));
    }
    Ok(())
}

struct SpawnBindingInput {
    alias: Ident,
    target: Ident,
}

impl Parse for SpawnBindingInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let alias = input.parse()?;
        input.parse::<Token![=>]>()?;
        let target = input.parse()?;

        Ok(Self { alias, target })
    }
}

struct ConfigValueInput {
    name: Ident,
    ty: Type,
    value: Expr,
}

impl Parse for ConfigValueInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![=]>()?;
        let value = input.parse()?;

        Ok(Self { name, ty, value })
    }
}

struct AppTask {
    name: Ident,
    binds: Option<Ident>,
    priority: LitInt,
    configs: Vec<ConfigValueInput>,
    bindings: Vec<SpawnBindingInput>,
}

impl Parse for AppTask {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;

        if !input.peek(syn::token::Brace) {
            return Err(Error::new(
                name.span(),
                "task priority must be declared in the app",
            ));
        }

        let body;
        braced!(body in input);

        let mut binds = None;
        let mut priority = None;
        let mut configs = Vec::new();
        let mut bindings = Vec::new();
        let mut has_config_section = false;
        let mut has_spawn_section = false;

        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "binds" => {
                    if binds.is_some() {
                        return Err(Error::new(
                            key.span(),
                            "hardware interrupt is bound more than once",
                        ));
                    }

                    binds = Some(body.parse()?);
                }
                "priority" => {
                    if priority.is_some() {
                        return Err(Error::new(
                            key.span(),
                            "priority is declared more than once",
                        ));
                    }

                    priority = Some(body.parse()?);
                }
                "config" => {
                    if has_config_section {
                        return Err(Error::new(key.span(), "config is declared more than once"));
                    }
                    has_config_section = true;

                    let config_body;
                    braced!(config_body in body);
                    configs =
                        Punctuated::<ConfigValueInput, Token![,]>::parse_terminated(&config_body)?
                            .into_iter()
                            .collect();
                }
                "spawn" => {
                    if has_spawn_section {
                        return Err(Error::new(key.span(), "spawn is declared more than once"));
                    }
                    has_spawn_section = true;

                    let spawn_body;
                    braced!(spawn_body in body);
                    bindings =
                        Punctuated::<SpawnBindingInput, Token![,]>::parse_terminated(&spawn_body)?
                            .into_iter()
                            .collect();
                }
                _ => {
                    return Err(Error::new(
                        key.span(),
                        "expected `binds`, `priority`, `config`, or `spawn`",
                    ));
                }
            }

            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            } else if !body.is_empty() {
                return Err(body.error("expected `,`"));
            }
        }

        let priority = priority
            .ok_or_else(|| Error::new(name.span(), "task priority must be declared in the app"))?;

        Ok(Self {
            name,
            binds,
            priority,
            configs,
            bindings,
        })
    }
}

struct AppInput {
    tasks: Vec<AppTask>,
}

impl Parse for AppInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let tasks = Punctuated::<AppTask, Token![,]>::parse_terminated(input)?;

        Ok(Self {
            tasks: tasks.into_iter().collect(),
        })
    }
}

struct ModuleAppInput {
    module: Ident,
    composition: AppInput,
}

struct RticAppInput {
    dependency_registry: Option<Path>,
    target: Option<TargetInput>,
    dispatchers: Vec<Ident>,
    dispatchers_span: Span,
    monotonic: Option<MonotonicInput>,
    app_module: Ident,
    tasks_module: Ident,
    shared: ItemStruct,
    local: ItemStruct,
    init: ItemFn,
    composition: AppInput,
}

struct TargetInput {
    mcu: Ident,
    hal: Path,
}

impl Parse for TargetInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let body;
        braced!(body in input);

        let mut mcu = None;
        let mut hal = None;

        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "mcu" => {
                    if mcu.is_some() {
                        return Err(Error::new(key.span(), "MCU is declared more than once"));
                    }
                    mcu = Some(body.parse()?);
                }
                "hal" => {
                    if hal.is_some() {
                        return Err(Error::new(key.span(), "HAL is declared more than once"));
                    }
                    hal = Some(body.parse()?);
                }
                _ => {
                    return Err(Error::new(
                        key.span(),
                        "expected `mcu` or `hal` in target declaration",
                    ));
                }
            }

            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            } else if !body.is_empty() {
                return Err(body.error("expected `,`"));
            }
        }

        Ok(Self {
            mcu: mcu
                .ok_or_else(|| Error::new(input.span(), "target declaration requires `mcu`"))?,
            hal: hal
                .ok_or_else(|| Error::new(input.span(), "target declaration requires `hal`"))?,
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
    fn parse(input: ParseStream) -> Result<Self> {
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
                    if source.is_some() {
                        return Err(Error::new(
                            key.span(),
                            "monotonic source is declared more than once",
                        ));
                    }

                    let source_name: Ident = body.parse()?;
                    source = Some(match source_name.to_string().as_str() {
                        "SysTick" => MonotonicSourceInput::SysTick,
                        "Timer" => {
                            let timer_body;
                            parenthesized!(timer_body in body);
                            let timer = timer_body.parse()?;

                            if !timer_body.is_empty() {
                                return Err(timer_body.error("expected one timer peripheral"));
                            }

                            MonotonicSourceInput::Timer(timer)
                        }
                        _ => {
                            return Err(Error::new(
                                source_name.span(),
                                "expected `SysTick` or `Timer(TIMx)`",
                            ));
                        }
                    });
                }
                "tick_hz" => {
                    if tick_hz.is_some() {
                        return Err(Error::new(
                            key.span(),
                            "monotonic tick frequency is declared more than once",
                        ));
                    }

                    let value: LitInt = body.parse()?;
                    let frequency = value.base10_parse::<u32>().map_err(|_| {
                        Error::new(value.span(), "monotonic tick frequency must fit in a u32")
                    })?;

                    if frequency == 0 {
                        return Err(Error::new(
                            value.span(),
                            "monotonic tick frequency must be greater than zero",
                        ));
                    }

                    tick_hz = Some(value);
                }
                _ => {
                    return Err(Error::new(
                        key.span(),
                        "expected `source` or `tick_hz` in monotonic declaration",
                    ));
                }
            }

            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            } else if !body.is_empty() {
                return Err(body.error("expected `,`"));
            }
        }

        Ok(Self {
            name,
            source: source.ok_or_else(|| {
                Error::new(input.span(), "monotonic declaration requires `source`")
            })?,
            tick_hz: tick_hz.ok_or_else(|| {
                Error::new(input.span(), "monotonic declaration requires `tick_hz`")
            })?,
        })
    }
}

enum AppMacroInput {
    Composition(AppInput),
    Module(ModuleAppInput),
    Rtic(Box<RticAppInput>),
}

impl Parse for AppMacroInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let dependency_registry = if input.peek(Ident) {
            let lookahead = input.fork();
            let key: Ident = lookahead.parse()?;

            if key == "dependency_registry" && lookahead.peek(Token![=]) {
                let key: Ident = input.parse()?;
                debug_assert_eq!(key, "dependency_registry");
                input.parse::<Token![=]>()?;
                let registry = input.parse()?;
                input.parse::<Token![,]>()?;
                Some(registry)
            } else {
                None
            }
        } else {
            None
        };

        let lookahead = input.fork();
        let has_target = if lookahead.peek(Ident) {
            let key: Ident = lookahead.parse()?;

            if key == "device" && lookahead.peek(Token![=]) {
                return Err(Error::new(
                    key.span(),
                    "`device` was replaced by `target = { mcu = ..., hal = ... }`",
                ));
            }

            key == "target" && lookahead.peek(Token![=])
        } else {
            false
        };

        let (target, dispatchers, dispatchers_span) = if has_target {
            let target_key: Ident = input.parse()?;
            debug_assert_eq!(target_key, "target");
            input.parse::<Token![=]>()?;
            let target = input.parse()?;
            input.parse::<Token![,]>()?;

            let dispatchers_key: Ident = input.parse()?;
            if dispatchers_key != "dispatchers" {
                return Err(Error::new(
                    dispatchers_key.span(),
                    "expected `dispatchers` after `target`",
                ));
            }
            input.parse::<Token![=]>()?;

            let dispatcher_body;
            bracketed!(dispatcher_body in input);
            let dispatchers = Punctuated::<Ident, Token![,]>::parse_terminated(&dispatcher_body)?
                .into_iter()
                .collect();
            input.parse::<Token![,]>()?;

            (Some(target), dispatchers, dispatchers_key.span())
        } else {
            (None, Vec::new(), input.span())
        };

        let monotonic = if has_target && input.peek(Ident) {
            let lookahead = input.fork();
            let key: Ident = lookahead.parse()?;

            if key == "monotonic" && lookahead.peek(Token![=]) {
                let key: Ident = input.parse()?;
                debug_assert_eq!(key, "monotonic");
                input.parse::<Token![=]>()?;
                let monotonic = input.parse()?;
                input.parse::<Token![,]>()?;
                Some(monotonic)
            } else {
                None
            }
        } else {
            None
        };

        if !input.peek(Token![mod]) {
            if has_target || dependency_registry.is_some() {
                return Err(input.error("expected `mod app { ... }` after app arguments"));
            }

            return Ok(Self::Composition(input.parse()?));
        }

        input.parse::<Token![mod]>()?;
        let module = input.parse()?;

        if input.peek(Token![;]) {
            if has_target || dependency_registry.is_some() {
                return Err(input.error("app arguments require the `mod app { ... }` form"));
            }

            input.parse::<Token![;]>()?;
            let composition = input.parse()?;

            return Ok(Self::Module(ModuleAppInput {
                module,
                composition,
            }));
        }

        let app_body;
        braced!(app_body in input);

        app_body.parse::<Token![mod]>().map_err(|_| {
            Error::new(
                app_body.span(),
                "expected a task module declaration such as `mod tasks;`",
            )
        })?;
        let tasks_module = app_body.parse()?;
        app_body.parse::<Token![;]>()?;

        let mut shared: ItemStruct = app_body.parse()?;
        let shared_span = shared.ident.span();
        remove_marker_attribute(&mut shared.attrs, "shared", shared_span)?;

        if shared.ident != "Shared" {
            return Err(Error::new(
                shared.ident.span(),
                "the `#[shared]` resource struct must be named `Shared`",
            ));
        }

        if !matches!(&shared.fields, Fields::Named(_)) {
            return Err(Error::new(
                shared.ident.span(),
                "`Shared` must use named fields",
            ));
        }

        let mut local: ItemStruct = app_body.parse()?;
        let local_span = local.ident.span();
        remove_marker_attribute(&mut local.attrs, "local", local_span)?;

        if local.ident != "Local" {
            return Err(Error::new(
                local.ident.span(),
                "the `#[local]` resource struct must be named `Local`",
            ));
        }

        if !matches!(&local.fields, Fields::Named(_)) {
            return Err(Error::new(
                local.ident.span(),
                "`Local` must use named fields",
            ));
        }

        let mut init: ItemFn = app_body.parse()?;
        let init_span = init.sig.ident.span();
        remove_marker_attribute(&mut init.attrs, "init", init_span)?;

        if init.sig.ident != "init" {
            return Err(Error::new(
                init.sig.ident.span(),
                "the `#[init]` function must be named `init`",
            ));
        }

        if init.sig.asyncness.is_some() {
            return Err(Error::new(
                init.sig.asyncness.span(),
                "the init task must be synchronous",
            ));
        }

        if init.sig.inputs.len() != 1 {
            return Err(Error::new(
                init.sig.inputs.span(),
                "init must accept exactly one `init::Context` argument",
            ));
        }

        let composition = app_body.parse()?;

        if !input.is_empty() {
            return Err(input.error("unexpected tokens after app module"));
        }

        Ok(Self::Rtic(Box::new(RticAppInput {
            dependency_registry,
            target,
            dispatchers,
            dispatchers_span,
            monotonic,
            app_module: module,
            tasks_module,
            shared,
            local,
            init,
            composition,
        })))
    }
}

#[proc_macro]
pub fn app(input: TokenStream) -> TokenStream {
    let app = parse_macro_input!(input as AppMacroInput);

    let output = match app {
        AppMacroInput::Composition(composition) => expand_app(composition),
        AppMacroInput::Module(module) => expand_module_app(module),
        AppMacroInput::Rtic(app) => expand_rtic_app(*app),
    };

    match output {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_module_app(app: ModuleAppInput) -> Result<TokenStream2> {
    if let Some(task) = app
        .composition
        .tasks
        .iter()
        .find(|task| task.binds.is_some())
    {
        return Err(Error::new(
            task.name.span(),
            "hardware tasks require the RTIC-shaped app form with a `target` argument",
        ));
    }

    let module = app.module;
    let task_names: Vec<_> = app
        .composition
        .tasks
        .iter()
        .map(|task| task.name.clone())
        .collect();
    let metadata = expand_app(app.composition)?;

    Ok(quote! {
        mod #module;

        use #module::{
            #(
                #task_names,
            )*
        };

        #metadata
    })
}

fn unique_priorities(app: &AppInput) -> Result<Vec<u8>> {
    let mut priorities = BTreeSet::new();

    for task in &app.tasks {
        let priority = task.priority.base10_parse::<u8>().map_err(|_| {
            Error::new(
                task.priority.span(),
                "task priority must be an integer from 0 through 255",
            )
        })?;

        priorities.insert(priority);
    }

    Ok(priorities.into_iter().collect())
}

fn unique_priorities_for<'a>(tasks: impl Iterator<Item = &'a AppTask>) -> Result<Vec<u8>> {
    let mut priorities = BTreeSet::new();

    for task in tasks {
        let priority = task.priority.base10_parse::<u8>().map_err(|_| {
            Error::new(
                task.priority.span(),
                "task priority must be an integer from 0 through 255",
            )
        })?;

        priorities.insert(priority);
    }

    Ok(priorities.into_iter().collect())
}

fn unique_software_priorities(app: &AppInput) -> Result<Vec<u8>> {
    unique_priorities_for(app.tasks.iter().filter(|task| task.binds.is_none()))
}

fn unique_hardware_priorities(app: &AppInput) -> Result<Vec<u8>> {
    unique_priorities_for(app.tasks.iter().filter(|task| task.binds.is_some()))
}

fn validate_interrupt_bindings(app: &AppInput, dispatchers: &[Ident]) -> Result<()> {
    let dispatcher_names: HashSet<_> = dispatchers.iter().map(ToString::to_string).collect();
    let mut hardware_interrupts = HashSet::new();

    for task in &app.tasks {
        let Some(interrupt) = &task.binds else {
            continue;
        };
        let interrupt_name = interrupt.to_string();

        if !hardware_interrupts.insert(interrupt_name.clone()) {
            return Err(Error::new(
                interrupt.span(),
                "hardware interrupt is bound to more than one task",
            ));
        }

        if dispatcher_names.contains(&interrupt_name) {
            return Err(Error::new(
                interrupt.span(),
                "hardware-task interrupt cannot also be used as a dispatcher",
            ));
        }
    }

    Ok(())
}

fn validate_dispatchers(
    app: &AppInput,
    dispatchers: &[Ident],
    dispatchers_span: Span,
) -> Result<Vec<u8>> {
    let mut dispatcher_names = HashSet::new();

    for dispatcher in dispatchers {
        if !dispatcher_names.insert(dispatcher.to_string()) {
            return Err(Error::new(
                dispatcher.span(),
                "dispatcher appears more than once",
            ));
        }
    }

    validate_interrupt_bindings(app, dispatchers)?;

    let priorities = unique_software_priorities(app)?;
    let dispatcher_priorities: Vec<_> = priorities
        .iter()
        .copied()
        .filter(|priority| *priority != 0)
        .collect();

    if dispatchers.len() < dispatcher_priorities.len() {
        return Err(Error::new(
            dispatchers_span,
            format!(
                "not enough dispatchers: app uses {} nonzero software-task priorities {:?}, but only {} dispatchers were provided",
                dispatcher_priorities.len(),
                dispatcher_priorities,
                dispatchers.len(),
            ),
        ));
    }

    Ok(priorities)
}

fn token_stream_contains_ident(tokens: TokenStream2, expected: &Ident) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == *expected,
        TokenTree::Group(group) => token_stream_contains_ident(group.stream(), expected),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetProperties {
    rust_target: &'static str,
    hal_feature: &'static str,
    rtic_monotonics_feature: &'static str,
    flash_origin: u32,
    flash_bytes: u32,
    ram_origin: u32,
    ram_bytes: u32,
}

fn resolve_target(target: &TargetInput) -> Result<TargetProperties> {
    match target.mcu.to_string().as_str() {
        "STM32F401RET6" => Ok(TargetProperties {
            rust_target: "thumbv7em-none-eabihf",
            hal_feature: "stm32f401",
            rtic_monotonics_feature: "stm32f401re",
            flash_origin: 0x0800_0000,
            flash_bytes: 512 * 1024,
            ram_origin: 0x2000_0000,
            ram_bytes: 96 * 1024,
        }),
        _ => Err(Error::new(
            target.mcu.span(),
            format!(
                "unsupported MCU `{}`; FerroForge currently supports `STM32F401RET6`",
                target.mcu
            ),
        )),
    }
}

fn validate_monotonic(
    monotonic: Option<&MonotonicInput>,
    dispatchers: &[Ident],
    app: &AppInput,
    local: &ItemStruct,
) -> Result<()> {
    let Some(MonotonicInput {
        source: MonotonicSourceInput::Timer(timer),
        ..
    }) = monotonic
    else {
        return Ok(());
    };

    if dispatchers.iter().any(|dispatcher| dispatcher == timer) {
        return Err(Error::new(
            timer.span(),
            "monotonic timer cannot also be used as a dispatcher",
        ));
    }

    if app
        .tasks
        .iter()
        .filter_map(|task| task.binds.as_ref())
        .any(|interrupt| interrupt == timer)
    {
        return Err(Error::new(
            timer.span(),
            "monotonic timer interrupt cannot also be bound to a hardware task",
        ));
    }

    let Fields::Named(local_fields) = &local.fields else {
        unreachable!("local fields were validated while parsing");
    };

    if local_fields.named.iter().any(|field| {
        let ty = &field.ty;
        token_stream_contains_ident(quote!(#ty), timer)
    }) {
        return Err(Error::new(
            timer.span(),
            "monotonic timer cannot also be declared as a local resource",
        ));
    }

    Ok(())
}

fn expand_rtic_app(app: RticAppInput) -> Result<TokenStream2> {
    let RticAppInput {
        dependency_registry,
        target,
        dispatchers,
        dispatchers_span,
        monotonic,
        app_module,
        tasks_module,
        shared,
        local,
        init,
        composition,
    } = app;
    let target_properties = target.as_ref().map(resolve_target).transpose()?;
    let has_target = target.is_some();
    validate_monotonic(monotonic.as_ref(), &dispatchers, &composition, &local)?;
    let priorities = unique_priorities(&composition)?;
    let software_priorities = if has_target {
        validate_dispatchers(&composition, &dispatchers, dispatchers_span)?
    } else {
        unique_software_priorities(&composition)?
    };
    let hardware_priorities = unique_hardware_priorities(&composition)?;

    if !has_target && composition.tasks.iter().any(|task| task.binds.is_some()) {
        return Err(Error::new(
            app_module.span(),
            "hardware tasks require a `target` app argument",
        ));
    }

    let unique_priority_count = priorities.len();
    let unique_software_priority_count = software_priorities.len();
    let unique_hardware_priority_count = hardware_priorities.len();
    let required_dispatchers = software_priorities
        .iter()
        .filter(|priority| **priority != 0)
        .count();
    let provided_dispatchers = dispatchers.len();
    let hardware_interrupts: Vec<_> = composition
        .tasks
        .iter()
        .filter_map(|task| task.binds.clone())
        .collect();
    let monotonic_interrupts: Vec<_> = monotonic
        .as_ref()
        .and_then(|monotonic| match &monotonic.source {
            MonotonicSourceInput::SysTick => None,
            MonotonicSourceInput::Timer(timer) => Some(timer.clone()),
        })
        .into_iter()
        .collect();
    let hardware_task_metadata: Vec<_> = composition
        .tasks
        .iter()
        .filter_map(|task| {
            let interrupt = task.binds.as_ref()?;
            let task_variant = variant_name(&task.name);
            let priority = &task.priority;

            Some(quote! {
                HardwareTaskMetadata {
                    task: Task::#task_variant,
                    interrupt: stringify!(#interrupt),
                    priority: #priority,
                }
            })
        })
        .collect();
    let init_context = if let Some(target) = target.as_ref() {
        let hal = &target.hal;

        quote! {
            pub(super) mod init {
                #[cfg(not(target_arch = "arm"))]
                #[derive(Debug, Clone, Copy, Default)]
                pub struct Context;

                #[cfg(target_arch = "arm")]
                pub struct Context {
                    pub device: #hal::pac::Peripherals,
                    pub core: ::cortex_m::Peripherals,
                }
            }
        }
    } else {
        quote! {
            pub(super) mod init {
                #[derive(Debug, Clone, Copy, Default)]
                pub struct Context;
            }
        }
    };
    let hal_prelude_import = target.as_ref().map(|target| {
        let hal = &target.hal;

        quote! {
            #[cfg(target_arch = "arm")]
            use #hal::prelude::*;
        }
    });
    let target_declarations = target.as_ref().map(|target| {
        let mcu = &target.mcu;
        let hal = &target.hal;
        let properties = target_properties.expect("a resolved target has target properties");
        let rust_target = properties.rust_target;
        let hal_feature = properties.hal_feature;
        let rtic_monotonics_feature = properties.rtic_monotonics_feature;
        let flash_origin = properties.flash_origin;
        let flash_bytes = properties.flash_bytes;
        let ram_origin = properties.ram_origin;
        let ram_bytes = properties.ram_bytes;

        quote! {
            #[cfg(target_arch = "arm")]
            mod __ferroforge_interrupt_checks {
                use #hal::pac as device;

                #(
                    const _: device::Interrupt = device::Interrupt::#dispatchers;
                )*
                #(
                    const _: device::Interrupt = device::Interrupt::#hardware_interrupts;
                )*
                #(
                    const _: device::Interrupt = device::Interrupt::#monotonic_interrupts;
                )*
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(super) struct MemoryRegionMetadata {
                pub(super) origin: u32,
                pub(super) size_bytes: u32,
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(super) struct TargetMetadata {
                pub(super) mcu: &'static str,
                pub(super) hal: &'static str,
                pub(super) rust_target: &'static str,
                pub(super) hal_feature: &'static str,
                pub(super) rtic_monotonics_feature: &'static str,
                pub(super) flash: MemoryRegionMetadata,
                pub(super) ram: MemoryRegionMetadata,
            }

            pub(super) const TARGET: TargetMetadata = TargetMetadata {
                mcu: stringify!(#mcu),
                hal: stringify!(#hal),
                rust_target: #rust_target,
                hal_feature: #hal_feature,
                rtic_monotonics_feature: #rtic_monotonics_feature,
                flash: MemoryRegionMetadata {
                    origin: #flash_origin,
                    size_bytes: #flash_bytes,
                },
                ram: MemoryRegionMetadata {
                    origin: #ram_origin,
                    size_bytes: #ram_bytes,
                },
            };

            pub(super) const DISPATCHERS: &[&str] = &[
                #(
                    stringify!(#dispatchers),
                )*
            ];

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(super) struct HardwareTaskMetadata {
                pub(super) task: Task,
                pub(super) interrupt: &'static str,
                pub(super) priority: u8,
            }

            pub(super) const HARDWARE_TASKS: &[HardwareTaskMetadata] = &[
                #(
                    #hardware_task_metadata,
                )*
            ];
        }
    });
    let task_names: Vec<_> = composition
        .tasks
        .iter()
        .map(|task| task.name.clone())
        .collect();
    let shared_check_tasks = task_names.clone();
    let Fields::Named(shared_fields) = &shared.fields else {
        unreachable!("shared fields were validated while parsing");
    };
    let shared_resource_variants: Vec<_> = shared_fields
        .named
        .iter()
        .map(|field| {
            variant_name(
                field
                    .ident
                    .as_ref()
                    .expect("named fields always have identifiers"),
            )
        })
        .collect();
    let shared_type_bindings: Vec<_> = task_names
        .iter()
        .flat_map(|task| {
            let target_cfg = has_target.then(|| quote!(#[cfg(target_arch = "arm")]));

            shared_fields.named.iter().map(move |field| {
                let resource = field
                    .ident
                    .as_ref()
                    .expect("named fields always have identifiers");
                let ty = &field.ty;
                let cfg_attrs: Vec<_> = field
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
                    .collect();

                quote! {
                    #target_cfg
                    #(#cfg_attrs)*
                    #task::__ferroforge_bind_shared!(#task, #resource, #ty);
                }
            })
        })
        .collect();
    let local_check_tasks = task_names.clone();
    let Fields::Named(local_fields) = &local.fields else {
        unreachable!("local fields were validated while parsing");
    };
    let local_resource_variants: Vec<_> = local_fields
        .named
        .iter()
        .map(|field| {
            variant_name(
                field
                    .ident
                    .as_ref()
                    .expect("named fields always have identifiers"),
            )
        })
        .collect();
    let local_type_bindings: Vec<_> = task_names
        .iter()
        .flat_map(|task| {
            let target_cfg = has_target.then(|| quote!(#[cfg(target_arch = "arm")]));

            local_fields.named.iter().map(move |field| {
                let resource = field
                    .ident
                    .as_ref()
                    .expect("named fields always have identifiers");
                let ty = &field.ty;
                let cfg_attrs: Vec<_> = field
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
                    .collect();

                quote! {
                    #target_cfg
                    #(#cfg_attrs)*
                    #task::__ferroforge_bind_local!(#task, #resource, #ty);
                }
            })
        })
        .collect();
    let monotonic_type_declaration = monotonic.as_ref().map(|monotonic| {
        let name = &monotonic.name;
        let tick_hz = &monotonic.tick_hz;

        quote! {
            pub(crate) type #name = ::ferroforge::MockMonotonic<{ #tick_hz }>;
        }
    });
    let monotonic_metadata_value = if let Some(monotonic) = monotonic.as_ref() {
        let name = &monotonic.name;
        let tick_hz = &monotonic.tick_hz;
        let source = match &monotonic.source {
            MonotonicSourceInput::SysTick => quote!(MonotonicSource::SysTick),
            MonotonicSourceInput::Timer(timer) => {
                quote!(MonotonicSource::Timer(stringify!(#timer)))
            }
        };

        quote! {
            ::core::option::Option::Some(MonotonicMetadata {
                name: stringify!(#name),
                source: #source,
                tick_hz: #tick_hz,
            })
        }
    } else {
        quote!(::core::option::Option::None)
    };
    let resource_declarations = if has_target {
        let shared_visibility = &shared.vis;
        let shared_name = &shared.ident;
        let local_visibility = &local.vis;
        let local_name = &local.ident;

        quote! {
            #[cfg(target_arch = "arm")]
            #shared

            #[cfg(not(target_arch = "arm"))]
            #shared_visibility struct #shared_name;

            #[cfg(target_arch = "arm")]
            #local

            #[cfg(not(target_arch = "arm"))]
            #local_visibility struct #local_name;
        }
    } else {
        quote! {
            #shared
            #local
        }
    };
    let init_declaration = if has_target {
        let init_attributes = &init.attrs;
        let init_visibility = &init.vis;
        let init_signature = &init.sig;

        quote! {
            #[cfg(target_arch = "arm")]
            #init

            #[cfg(not(target_arch = "arm"))]
            #(#init_attributes)*
            #[allow(unused_variables)]
            #init_visibility #init_signature {
                ::core::panic!("firmware initialization is not executable in the host mock")
            }
        }
    } else {
        quote!(#init)
    };
    let run_init = if has_target {
        quote! {
            #[cfg(not(target_arch = "arm"))]
            pub(super) fn run_init() -> impl ::core::marker::Sized {
                init(init::Context::default())
            }
        }
    } else {
        quote! {
            pub(super) fn run_init() -> impl ::core::marker::Sized {
                init(init::Context::default())
            }
        }
    };
    let renderer_target = if let Some(target) = target.as_ref() {
        let mcu = &target.mcu;
        let hal = &target.hal;
        let properties = target_properties.expect("a resolved target has target properties");
        let rust_target = properties.rust_target;
        let hal_feature = properties.hal_feature;
        let rtic_monotonics_feature = properties.rtic_monotonics_feature;
        let flash_origin = properties.flash_origin;
        let flash_bytes = properties.flash_bytes;
        let ram_origin = properties.ram_origin;
        let ram_bytes = properties.ram_bytes;

        quote! {
            ::core::option::Option::Some(::ferroforge::TargetDefinition {
                mcu: stringify!(#mcu),
                hal: stringify!(#hal),
                rust_target: #rust_target,
                hal_feature: #hal_feature,
                rtic_monotonics_feature: #rtic_monotonics_feature,
                flash: ::ferroforge::MemoryRegionDefinition {
                    origin: #flash_origin,
                    size_bytes: #flash_bytes,
                },
                ram: ::ferroforge::MemoryRegionDefinition {
                    origin: #ram_origin,
                    size_bytes: #ram_bytes,
                },
            })
        }
    } else {
        quote!(::core::option::Option::None)
    };
    let renderer_monotonic = if let Some(monotonic) = monotonic.as_ref() {
        let name = &monotonic.name;
        let tick_hz = &monotonic.tick_hz;
        let source = match &monotonic.source {
            MonotonicSourceInput::SysTick => {
                quote!(::ferroforge::MonotonicSourceDefinition::SysTick)
            }
            MonotonicSourceInput::Timer(timer) => quote! {
                ::ferroforge::MonotonicSourceDefinition::Timer(stringify!(#timer))
            },
        };

        quote! {
            ::core::option::Option::Some(::ferroforge::MonotonicDefinition {
                name: stringify!(#name),
                source: #source,
                tick_hz: #tick_hz,
            })
        }
    } else {
        quote!(::core::option::Option::None)
    };
    let renderer_tasks = composition.tasks.iter().map(|task| {
        let name = &task.name;
        let priority = &task.priority;
        let interrupt = if let Some(interrupt) = task.binds.as_ref() {
            quote!(::core::option::Option::Some(stringify!(#interrupt)))
        } else {
            quote!(::core::option::Option::None)
        };
        let configuration = task.configs.iter().map(|config| {
            let config_name = &config.name;
            let ty = &config.ty;
            let value = &config.value;

            quote! {
                ::ferroforge::TaskConfigurationDefinition {
                    name: stringify!(#config_name),
                    rust_type: stringify!(#ty),
                    value: stringify!(#value),
                }
            }
        });
        let spawn_bindings = task.bindings.iter().map(|binding| {
            let alias = &binding.alias;
            let target = &binding.target;

            quote! {
                ::ferroforge::TaskSpawnBindingDefinition {
                    alias: stringify!(#alias),
                    target: stringify!(#target),
                }
            }
        });

        quote! {
            ::ferroforge::TaskDefinition {
                name: stringify!(#name),
                priority: #priority,
                interrupt: #interrupt,
                implementation: #name::IMPLEMENTATION,
                configuration: &[#(#configuration),*],
                spawn_bindings: &[#(#spawn_bindings),*],
            }
        }
    });
    let renderer_dependencies = dependency_registry
        .as_ref()
        .map(|registry| quote!(#registry::DEPENDENCY_CATALOG))
        .unwrap_or_else(|| quote!(&[]));
    let renderer_application = quote! {
        pub const APPLICATION: ::ferroforge::ApplicationDefinition =
            ::ferroforge::ApplicationDefinition {
                manifest_dir: env!("CARGO_MANIFEST_DIR"),
                source_file: file!(),
                module: stringify!(#app_module),
                target: #renderer_target,
                dispatchers: &[#(stringify!(#dispatchers)),*],
                monotonic: #renderer_monotonic,
                shared_source: stringify!(#shared),
                local_source: stringify!(#local),
                init_source: stringify!(#init),
                tasks: &[#(#renderer_tasks),*],
                dependencies: #renderer_dependencies,
            };
    };
    let metadata = expand_app_with_visibility(
        composition,
        quote!(pub(super)),
        dependency_registry.as_ref(),
    )?;

    Ok(quote! {
        #monotonic_type_declaration

        mod #tasks_module;

        pub mod #app_module {
            #hal_prelude_import

            #[allow(unused_imports)]
            use super::*;

            pub(super) use super::#tasks_module::{
                #(
                    #task_names,
                )*
            };

            #target_declarations

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(super) enum MonotonicSource {
                SysTick,
                Timer(&'static str),
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(super) struct MonotonicMetadata {
                pub(super) name: &'static str,
                pub(super) source: MonotonicSource,
                pub(super) tick_hz: u32,
            }

            pub(super) const MONOTONIC: ::core::option::Option<MonotonicMetadata> =
                #monotonic_metadata_value;

            pub(super) const PRIORITIES: &[u8] = &[
                #(
                    #priorities,
                )*
            ];
            pub(super) const SOFTWARE_PRIORITIES: &[u8] = &[
                #(
                    #software_priorities,
                )*
            ];
            pub(super) const HARDWARE_PRIORITIES: &[u8] = &[
                #(
                    #hardware_priorities,
                )*
            ];
            pub(super) const UNIQUE_PRIORITY_COUNT: usize = #unique_priority_count;
            pub(super) const UNIQUE_SOFTWARE_PRIORITY_COUNT: usize =
                #unique_software_priority_count;
            pub(super) const UNIQUE_HARDWARE_PRIORITY_COUNT: usize =
                #unique_hardware_priority_count;
            pub(super) const REQUIRED_DISPATCHERS: usize = #required_dispatchers;
            pub(super) const PROVIDED_DISPATCHERS: usize = #provided_dispatchers;

            #resource_declarations

            #init_context

            #init_declaration

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            enum SharedResource {
                #(
                    #shared_resource_variants,
                )*
            }

            #(
                #shared_check_tasks::__ferroforge_check_shared!(SharedResource);
            )*

            #(
                #shared_type_bindings
            )*

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            enum LocalResource {
                #(
                    #local_resource_variants,
                )*
            }

            #(
                #local_check_tasks::__ferroforge_check_local!(LocalResource);
            )*

            #(
                #local_type_bindings
            )*

            #metadata

            #renderer_application

            #run_init
        }
    })
}

fn expand_app(app: AppInput) -> Result<TokenStream2> {
    if let Some(task) = app.tasks.iter().find(|task| task.binds.is_some()) {
        return Err(Error::new(
            task.name.span(),
            "hardware tasks require the RTIC-shaped app form with a `target` argument",
        ));
    }

    expand_app_with_visibility(app, quote!(), None)
}

struct CompositionInput {
    dispatchers: Vec<Ident>,
    app: AppInput,
}

impl Parse for CompositionInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        if key != "dispatchers" {
            return Err(Error::new(key.span(), "expected `dispatchers`"));
        }
        input.parse::<Token![=]>()?;

        let body;
        bracketed!(body in input);
        let dispatchers = Punctuated::<Ident, Token![,]>::parse_terminated(&body)?
            .into_iter()
            .collect();
        input.parse::<Token![,]>()?;

        Ok(Self {
            dispatchers,
            app: input.parse()?,
        })
    }
}

/// Declares the host-owned scheduling and configuration for embedded sources.
#[proc_macro]
pub fn composition(input: TokenStream) -> TokenStream {
    let composition = parse_macro_input!(input as CompositionInput);

    match expand_composition(composition) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_composition(composition: CompositionInput) -> Result<TokenStream2> {
    validate_interrupt_bindings(&composition.app, &composition.dispatchers)?;

    let mut task_names = HashSet::new();
    for task in &composition.app.tasks {
        if !task_names.insert(task.name.to_string()) {
            return Err(Error::new(
                task.name.span(),
                "task appears more than once in composition",
            ));
        }
    }

    let dispatchers = &composition.dispatchers;
    let tasks = composition.app.tasks.iter().map(|task| {
        let name = &task.name;
        let priority = &task.priority;
        let interrupt = task
            .binds
            .as_ref()
            .map(|interrupt| quote!(::core::option::Option::Some(stringify!(#interrupt))))
            .unwrap_or_else(|| quote!(::core::option::Option::None));
        let configuration = task.configs.iter().map(|configuration| {
            let name = &configuration.name;
            let ty = &configuration.ty;
            let value = &configuration.value;

            quote! {
                ::ferroforge::TaskConfigurationDefinition {
                    name: stringify!(#name),
                    rust_type: stringify!(#ty),
                    value: stringify!(#value),
                }
            }
        });
        let spawn_bindings = task.bindings.iter().map(|binding| {
            let alias = &binding.alias;
            let target = &binding.target;

            quote! {
                ::ferroforge::TaskSpawnBindingDefinition {
                    alias: stringify!(#alias),
                    target: stringify!(#target),
                }
            }
        });

        quote! {
            ::ferroforge::ComposedTaskDefinition {
                name: stringify!(#name),
                priority: #priority,
                interrupt: #interrupt,
                configuration: &[#(#configuration),*],
                spawn_bindings: &[#(#spawn_bindings),*],
            }
        }
    });

    Ok(quote! {
        ::ferroforge::CompositionDefinition {
            dispatchers: &[#(stringify!(#dispatchers)),*],
            tasks: &[#(#tasks),*],
        }
    })
}

fn expand_app_with_visibility(
    app: AppInput,
    visibility: TokenStream2,
    dependency_registry: Option<&Path>,
) -> Result<TokenStream2> {
    validate_interrupt_bindings(&app, &[])?;

    let mut task_names = HashSet::new();

    for task in &app.tasks {
        if !task_names.insert(task.name.to_string()) {
            return Err(Error::new(
                task.name.span(),
                "task appears more than once in app",
            ));
        }
    }

    let hardware_task_names: HashSet<_> = app
        .tasks
        .iter()
        .filter(|task| task.binds.is_some())
        .map(|task| task.name.to_string())
        .collect();

    for task in &app.tasks {
        let mut config_names = HashSet::new();

        for config in &task.configs {
            if !config_names.insert(config.name.to_string()) {
                return Err(Error::new(
                    config.name.span(),
                    "task configuration is assigned more than once",
                ));
            }
        }

        let mut aliases = HashSet::new();

        for binding in &task.bindings {
            if !aliases.insert(binding.alias.to_string()) {
                return Err(Error::new(
                    binding.alias.span(),
                    "spawn alias is bound more than once",
                ));
            }

            if !task_names.contains(&binding.target.to_string()) {
                return Err(Error::new(
                    binding.target.span(),
                    "spawn target is not part of this app",
                ));
            }

            if hardware_task_names.contains(&binding.target.to_string()) {
                return Err(Error::new(
                    binding.target.span(),
                    "hardware tasks cannot be spawned; pend their interrupt instead",
                ));
            }
        }
    }

    let task_variants = app.tasks.iter().map(|task| variant_name(&task.name));
    let task_metadata = app.tasks.iter().map(|task| {
        let task_variant = variant_name(&task.name);
        let priority = &task.priority;
        let kind = if task.binds.is_some() {
            quote!(TaskKind::Hardware)
        } else {
            quote!(TaskKind::Software)
        };

        quote! {
            TaskMetadata {
                task: Task::#task_variant,
                priority: #priority,
                kind: #kind,
            }
        }
    });
    let dependency_metadata = if let Some(registry) = dependency_registry {
        let checks = app.tasks.iter().map(|task| {
            let task_name = &task.name;

            quote! {
                #task_name::__ferroforge_check_dependencies!(DependencyId);
            }
        });
        let task_requirements = app.tasks.iter().map(|task| {
            let task_name = &task.name;
            let task_variant = variant_name(task_name);

            quote! {
                TaskDependencyMetadata {
                    task: Task::#task_variant,
                    requirements:
                        #task_name::__ferroforge_dependency_requirements!(DependencyId),
                }
            }
        });
        let dependency_count_terms = app.tasks.iter().map(|task| {
            let task_name = &task.name;
            quote!(+ #task_name::DEPENDENCY_COUNT)
        });

        quote! {
            #visibility use #registry::DependencyId;
            #visibility type DependencyDefinition =
                ::ferroforge::DependencyDefinition<DependencyId>;
            #visibility type DependencyRequirement =
                ::ferroforge::DependencyRequirement<DependencyId>;

            #(#checks)*

            #visibility const DEPENDENCY_REGISTRY: &[DependencyDefinition] =
                #registry::DEPENDENCY_REGISTRY;

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #visibility struct TaskDependencyMetadata {
                #visibility task: Task,
                #visibility requirements: &'static [DependencyRequirement],
            }

            #visibility const TASK_DEPENDENCIES: &[TaskDependencyMetadata] = &[
                #(#task_requirements),*
            ];

            #visibility const TASK_DEPENDENCY_COUNT: usize =
                0 #(#dependency_count_terms)*;
        }
    } else {
        let no_dependency_checks = app.tasks.iter().map(|task| {
            let task_name = &task.name;

            quote! {
                #task_name::__ferroforge_require_no_dependencies!();
            }
        });
        let empty_task_requirements = app.tasks.iter().map(|task| {
            let task_variant = variant_name(&task.name);

            quote! {
                TaskDependencyMetadata {
                    task: Task::#task_variant,
                    requirements: &[],
                }
            }
        });

        quote! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #visibility enum DependencyId {}

            #visibility type DependencyDefinition =
                ::ferroforge::DependencyDefinition<DependencyId>;
            #visibility type DependencyRequirement =
                ::ferroforge::DependencyRequirement<DependencyId>;

            #(#no_dependency_checks)*

            #visibility const DEPENDENCY_REGISTRY: &[DependencyDefinition] = &[];

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #visibility struct TaskDependencyMetadata {
                #visibility task: Task,
                #visibility requirements: &'static [DependencyRequirement],
            }

            #visibility const TASK_DEPENDENCIES: &[TaskDependencyMetadata] = &[
                #(#empty_task_requirements),*
            ];

            #visibility const TASK_DEPENDENCY_COUNT: usize = 0;
        }
    };
    let hardware_task_shape_checks = app.tasks.iter().filter_map(|task| {
        task.binds.as_ref()?;
        let task_name = &task.name;

        Some(quote! {
            const _: () = {
                assert!(
                    !#task_name::IS_ASYNC,
                    concat!(
                        "hardware task `",
                        stringify!(#task_name),
                        "` must be synchronous",
                    ),
                );
                assert!(
                    #task_name::ARGUMENT_COUNT == 1,
                    concat!(
                        "hardware task `",
                        stringify!(#task_name),
                        "` must accept exactly one Context argument",
                    ),
                );
            };
        })
    });
    let config_impls = app
        .tasks
        .iter()
        .filter(|task| !task.configs.is_empty())
        .map(|task| {
            let task_name = &task.name;
            let constants = task.configs.iter().map(|config| {
                let name = const_name(&config.name);
                let ty = &config.ty;
                let value = &config.value;

                quote! {
                    pub(crate) const #name: #ty = #value;
                }
            });

            quote! {
                impl #task_name::Config {
                    #(
                        #constants
                    )*
                }
            }
        });
    let config_key_checks = app.tasks.iter().flat_map(|task| {
        let task_name = &task.name;

        task.configs.iter().map(move |config| {
            let variant = variant_name(&config.name);

            quote! {
                const _: #task_name::ConfigKey = #task_name::ConfigKey::#variant;
            }
        })
    });
    let spawn_source_variants = app
        .tasks
        .iter()
        .filter(|task| !task.bindings.is_empty())
        .map(|task| {
            let task_name = &task.name;
            let variant = variant_name(task_name);

            quote! {
                #variant(#task_name::Spawn)
            }
        });
    let bindings = app.tasks.iter().flat_map(|task| {
        let source_task = &task.name;
        let source_variant = variant_name(source_task);

        task.bindings.iter().map(move |binding| {
            let alias_variant = variant_name(&binding.alias);
            let target_variant = variant_name(&binding.target);

            quote! {
                SpawnBinding {
                    source: SpawnSource::#source_variant(
                        #source_task::Spawn::#alias_variant
                    ),
                    target: Task::#target_variant,
                }
            }
        })
    });
    let spawn_methods = app.tasks.iter().flat_map(|task| {
        let source_task = &task.name;

        task.bindings.iter().map(move |binding| {
            let alias = &binding.alias;
            let target_task = &binding.target;

            quote! {
                #target_task::__ferroforge_define_spawn!(
                    #source_task::SpawnHandle,
                    #alias
                );
            }
        })
    });
    let spawn_count_checks = app.tasks.iter().map(|task| {
        let task_name = &task.name;
        let count = task.bindings.len();

        quote! {
            const _: [(); #task_name::SPAWN_COUNT] = [(); #count];
        }
    });
    let config_count_checks = app.tasks.iter().map(|task| {
        let task_name = &task.name;
        let count = task.configs.len();

        quote! {
            const _: [(); #task_name::CONFIG_COUNT] = [(); #count];
        }
    });

    Ok(quote! {
        #(
            #config_impls
        )*

        #(
            #spawn_methods
        )*

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #visibility enum Task {
            #(
                #task_variants,
            )*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #visibility enum TaskKind {
            Software,
            Hardware,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #visibility struct TaskMetadata {
            #visibility task: Task,
            #visibility priority: u8,
            #visibility kind: TaskKind,
        }

        #visibility const TASKS: &[TaskMetadata] = &[
            #(
                #task_metadata,
            )*
        ];

        #dependency_metadata

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #visibility enum SpawnSource {
            #(
                #spawn_source_variants,
            )*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #visibility struct SpawnBinding {
            #visibility source: SpawnSource,
            #visibility target: Task,
        }

        #visibility const SPAWN_BINDINGS: &[SpawnBinding] = &[
            #(
                #bindings,
            )*
        ];

        #(
            #spawn_count_checks
        )*

        #(
            #config_count_checks
        )*

        #(
            #config_key_checks
        )*

        #(
            #hardware_task_shape_checks
        )*
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_app(tasks: &str) -> AppInput {
        syn::parse_str(tasks).expect("test app should parse")
    }

    fn parse_rtic_app(source: &str) -> RticAppInput {
        match syn::parse_str::<AppMacroInput>(source).expect("test RTIC app should parse") {
            AppMacroInput::Rtic(app) => *app,
            _ => panic!("expected the RTIC-shaped app form"),
        }
    }

    #[test]
    fn parses_task_dependency_features() {
        let args: TaskArgs =
            syn::parse_str("dependencies = [Heapless, Serde(features = [\"derive\", \"alloc\"]) ]")
                .expect("task dependencies should parse");

        assert_eq!(args.dependencies.len(), 2);
        assert_eq!(args.dependencies[0].id, "Heapless");
        assert!(args.dependencies[0].features.is_empty());
        assert_eq!(args.dependencies[1].id, "Serde");
        assert_eq!(args.dependencies[1].features[0].value(), "derive");
        assert_eq!(args.dependencies[1].features[1].value(), "alloc");
    }

    #[test]
    fn expands_host_composition_as_renderer_data() {
        let composition: CompositionInput = syn::parse_str(
            "dispatchers = [USART1],
             blink {
                 priority = 1,
                 config = { period_ms: u64 = 500 },
             },
             timer_interrupt {
                 binds = TIM2,
                 priority = 2,
                 config = {
                     frequency_hz: u32 = 1,
                     message: &'static str = \"Hello World\",
                 },
             }",
        )
        .expect("host composition should parse");

        let output = expand_composition(composition)
            .expect("host composition should expand")
            .to_string();

        assert!(output.contains("CompositionDefinition"));
        assert!(output.contains("dispatchers : & [stringify ! (USART1)]"));
        assert!(output.contains("name : stringify ! (blink)"));
        assert!(output.contains("priority : 2"));
        assert!(output.contains("Option :: Some (stringify ! (TIM2))"));
        assert!(output.contains("period_ms"));
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn rejects_duplicate_task_dependencies() {
        let error = syn::parse_str::<TaskArgs>("dependencies = [Heapless, Heapless]")
            .err()
            .expect("shared parser must reject duplicate dependencies");

        assert_eq!(error.to_string(), "duplicate task dependency");
    }

    #[test]
    fn task_body_is_emitted_once_with_target_specific_contexts() {
        let args: TaskArgs = syn::parse_str("local = [led]").expect("task arguments should parse");
        let function: ItemFn =
            syn::parse_str("async fn blink(cx: blink::Context) { cx.local.led.toggle(); }")
                .expect("task should parse");

        let output = expand_task(args, function)
            .expect("task should expand")
            .to_string();

        assert!(output.contains("target_arch"));
        assert!(!output.contains("firmware task bodies are not executable"));
        assert_eq!(output.matches("cx . local . led . toggle").count(), 2);
    }

    #[test]
    fn expands_dependency_registry_metadata() {
        let registry: DependencyRegistryInput = syn::parse_str(
            "Heapless => {
                package = \"heapless\",
                version = \"0.9\",
                default_features = false,
                features = [\"serde\"],
            },
            Serde => {
                package = \"serde\",
                version = \"1\",
            }",
        )
        .expect("registry should parse");

        assert!(!registry.entries[0].default_features.value);
        assert!(registry.entries[1].default_features.value);

        let output = expand_dependency_registry(registry)
            .expect("unique dependency declarations should expand")
            .to_string();

        assert!(output.contains("enum DependencyId"));
        assert!(output.contains("Heapless"));
        assert!(output.contains("DEPENDENCY_REGISTRY"));
        assert!(output.contains("heapless"));
    }

    #[test]
    fn rejects_duplicate_dependency_registry_ids() {
        let registry: DependencyRegistryInput = syn::parse_str(
            "Heapless => { package = \"heapless\", version = \"0.9\" },
             Heapless => { package = \"heapless-v2\", version = \"0.10\" }",
        )
        .expect("duplicates should parse before validation");

        let error = expand_dependency_registry(registry).expect_err("registry IDs must be unique");

        assert_eq!(
            error.to_string(),
            "dependency ID is declared more than once"
        );
    }

    #[test]
    fn rejects_duplicate_dependency_registry_packages() {
        let registry: DependencyRegistryInput = syn::parse_str(
            "Heapless => { package = \"heapless\", version = \"0.9\" },
             HeaplessV2 => { package = \"heapless\", version = \"0.10\" }",
        )
        .expect("duplicates should parse before validation");

        let error =
            expand_dependency_registry(registry).expect_err("registry packages must be unique");

        assert_eq!(
            error.to_string(),
            "dependency package is declared more than once"
        );
    }

    #[test]
    fn parses_dependency_registry_app_argument() {
        let app = parse_rtic_app(
            "dependency_registry = crate::dependencies,
             mod app {
                 mod tasks;
                 #[shared] struct Shared {}
                 #[local] struct Local {}
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { loop {} }
             }",
        );

        assert_eq!(
            app.dependency_registry
                .expect("registry should be recorded")
                .segments
                .last()
                .expect("registry path should not be empty")
                .ident,
            "dependencies"
        );
    }

    #[test]
    fn resolves_stm32f401ret6_target_properties() {
        let target: TargetInput = syn::parse_str("{ mcu = STM32F401RET6, hal = stm32f4xx_hal }")
            .expect("supported target declaration should parse");

        assert_eq!(
            resolve_target(&target).expect("STM32F401RET6 should be supported"),
            TargetProperties {
                rust_target: "thumbv7em-none-eabihf",
                hal_feature: "stm32f401",
                rtic_monotonics_feature: "stm32f401re",
                flash_origin: 0x0800_0000,
                flash_bytes: 512 * 1024,
                ram_origin: 0x2000_0000,
                ram_bytes: 96 * 1024,
            }
        );
    }

    #[test]
    fn rejects_unsupported_mcu_target() {
        let target: TargetInput =
            syn::parse_str("{ mcu = STM32_NOT_SUPPORTED, hal = stm32f4xx_hal }")
                .expect("target declaration should parse before resolution");

        let error = resolve_target(&target).expect_err("unknown MCUs must not be guessed");

        assert!(error.to_string().contains("unsupported MCU"));
        assert!(error.to_string().contains("STM32F401RET6"));
    }

    #[test]
    fn rejects_removed_device_argument() {
        let result = syn::parse_str::<AppMacroInput>(
            "device = stm32f4xx_hal::pac,
             dispatchers = [],
             mod app {
                 mod tasks;
                 #[shared] struct Shared {}
                 #[local] struct Local {}
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { loop {} }
             }",
        );
        let Err(error) = result else {
            panic!("the old device argument should not be accepted");
        };

        assert!(
            error
                .to_string()
                .contains("`device` was replaced by `target")
        );
    }

    #[test]
    fn rejects_too_few_dispatchers_for_unique_nonzero_priorities() {
        let app = parse_app("low { priority = 1 }, high { priority = 2 }");
        let dispatchers = [Ident::new("SWI0", Span::call_site())];

        let error = validate_dispatchers(&app, &dispatchers, Span::call_site())
            .expect_err("one dispatcher cannot cover two priorities");

        assert!(error.to_string().contains("not enough dispatchers"));
        assert!(error.to_string().contains("[1, 2]"));
    }

    #[test]
    fn rejects_duplicate_dispatchers() {
        let app = parse_app("low { priority = 1 }");
        let dispatchers = [
            Ident::new("SWI0", Span::call_site()),
            Ident::new("SWI0", Span::call_site()),
        ];

        let error = validate_dispatchers(&app, &dispatchers, Span::call_site())
            .expect_err("dispatcher names must be unique");

        assert_eq!(error.to_string(), "dispatcher appears more than once");
    }

    #[test]
    fn repeated_task_priorities_share_one_dispatcher() {
        let app = parse_app("first { priority = 1 }, second { priority = 1 }");
        let dispatchers = [Ident::new("SWI0", Span::call_site())];

        let priorities = validate_dispatchers(&app, &dispatchers, Span::call_site())
            .expect("one unique priority needs one dispatcher");

        assert_eq!(priorities, vec![1]);
    }

    #[test]
    fn hardware_priorities_do_not_consume_dispatchers() {
        let app = parse_app("software { priority = 1 }, timer { binds = TIM2, priority = 2 }");
        let dispatchers = [Ident::new("USART1", Span::call_site())];

        let priorities = validate_dispatchers(&app, &dispatchers, Span::call_site())
            .expect("only the software priority needs a dispatcher");

        assert_eq!(priorities, vec![1]);
    }

    #[test]
    fn rejects_duplicate_hardware_interrupt_bindings() {
        let app = parse_app(
            "first { binds = TIM2, priority = 1 }, second { binds = TIM2, priority = 2 }",
        );

        let error = validate_interrupt_bindings(&app, &[])
            .expect_err("a hardware interrupt can only bind one task");

        assert_eq!(
            error.to_string(),
            "hardware interrupt is bound to more than one task"
        );
    }

    #[test]
    fn rejects_hardware_interrupt_used_as_dispatcher() {
        let app = parse_app("timer { binds = TIM2, priority = 2 }");
        let dispatchers = [Ident::new("TIM2", Span::call_site())];

        let error = validate_dispatchers(&app, &dispatchers, Span::call_site())
            .expect_err("a bound interrupt cannot dispatch software tasks");

        assert_eq!(
            error.to_string(),
            "hardware-task interrupt cannot also be used as a dispatcher"
        );
    }

    #[test]
    fn rejects_hardware_task_as_spawn_target() {
        let app = parse_app(
            "timer { binds = TIM2, priority = 2 }, source { priority = 1, spawn = { timer => timer } }",
        );

        let error = expand_app_with_visibility(app, quote!(), None)
            .expect_err("hardware tasks are pended rather than spawned");

        assert_eq!(
            error.to_string(),
            "hardware tasks cannot be spawned; pend their interrupt instead"
        );
    }

    #[test]
    fn accepts_systick_monotonic() {
        let app = parse_rtic_app(
            "target = { mcu = STM32F401RET6, hal = hal },
             dispatchers = [],
             monotonic = Mono { source = SysTick, tick_hz = 1_000 },
             mod app {
                 mod tasks;
                 #[shared] struct Shared {}
                 #[local] struct Local {}
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { loop {} }
             }",
        );

        expand_rtic_app(app).expect("SysTick should be accepted as a monotonic source");
    }

    #[test]
    fn target_app_expansion_contains_host_and_arm_contexts() {
        let app = parse_rtic_app(
            "target = { mcu = STM32F401RET6, hal = hal },
             dispatchers = [],
             mod app {
                 mod tasks;
                 #[shared] struct Shared { enabled: bool }
                 #[local] struct Local { led: hal::Led }
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { todo!() }
             }",
        );

        let output = expand_rtic_app(app)
            .expect("target app should expand")
            .to_string();

        assert!(output.contains("target_arch"));
        assert!(output.contains("struct Shared ;"));
        assert!(output.contains("struct Local ;"));
        assert!(output.contains("firmware initialization is not executable"));
        assert!(output.contains("enabled : bool"));
        assert!(output.contains("led : hal :: Led"));
    }

    #[test]
    fn rejects_monotonic_timer_used_as_dispatcher() {
        let app = parse_rtic_app(
            "target = { mcu = STM32F401RET6, hal = hal },
             dispatchers = [TIM5],
             monotonic = Mono { source = Timer(TIM5), tick_hz = 1_000 },
             mod app {
                 mod tasks;
                 #[shared] struct Shared {}
                 #[local] struct Local {}
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { loop {} }
             }",
        );

        let error = expand_rtic_app(app).expect_err("TIM5 must be reserved for the monotonic");

        assert_eq!(
            error.to_string(),
            "monotonic timer cannot also be used as a dispatcher"
        );
    }

    #[test]
    fn rejects_monotonic_timer_bound_to_hardware_task() {
        let app = parse_rtic_app(
            "target = { mcu = STM32F401RET6, hal = hal },
             dispatchers = [],
             monotonic = Mono { source = Timer(TIM5), tick_hz = 1_000 },
             mod app {
                 mod tasks;
                 #[shared] struct Shared {}
                 #[local] struct Local {}
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { loop {} }
                 timer { binds = TIM5, priority = 1 },
             }",
        );

        let error = expand_rtic_app(app).expect_err("TIM5 cannot bind a second interrupt user");

        assert_eq!(
            error.to_string(),
            "monotonic timer interrupt cannot also be bound to a hardware task"
        );
    }

    #[test]
    fn rejects_monotonic_timer_declared_as_local_resource() {
        let app = parse_rtic_app(
            "target = { mcu = STM32F401RET6, hal = hal },
             dispatchers = [],
             monotonic = Mono { source = Timer(TIM5), tick_hz = 1_000 },
             mod app {
                 mod tasks;
                 #[shared] struct Shared {}
                 #[local] struct Local { timer: hal::CounterHz<hal::pac::TIM5> }
                 #[init] fn init(_cx: init::Context) -> (Shared, Local) { loop {} }
             }",
        );

        let error = expand_rtic_app(app).expect_err("TIM5 cannot also be a local resource");

        assert_eq!(
            error.to_string(),
            "monotonic timer cannot also be declared as a local resource"
        );
    }
}
