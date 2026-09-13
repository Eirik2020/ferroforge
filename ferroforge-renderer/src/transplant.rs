//! Standalone source-to-RTIC transplantation.
//!
//! The bounded path retains renderer-owned task namespaces, applies the
//! supported composition mappings, and can transplant an independently checked
//! system init package into the real RTIC app. Manifest generation and complete
//! host orchestration remain separate later stages.

use std::collections::{BTreeMap, BTreeSet};

use ferroforge_contracts::identifier_key;
use heck::ToShoutySnakeCase;
use proc_macro2::{TokenStream, TokenTree};
use quote::quote;
use syn::{
    AttrStyle, Attribute, Expr, ExprField, File, FnArg, Ident, Item, ItemFn, ItemStruct, Member,
    PatIdent, Path, Token, Type, Visibility, parse::Parser, parse_quote, punctuated::Punctuated,
    spanned::Spanned, visit_mut::VisitMut,
};

use crate::{
    RenderError,
    composition::{
        INITIAL_SYSTICK_COUNTER_BITS, INITIAL_SYSTICK_TICK_HZ, MonotonicProfile, MonotonicSource,
        ValidatedComposition, ValidatedTask,
    },
    source::{InitPackage, ModuleId, ModuleSource, TaskSources},
};

/// Compatibility shell for the early task-layout entry point.
///
/// Every source fragment is parsed before emission. New code that has a
/// discovered init package should use [`RticAppTarget`] and
/// [`render_rtic_app_with_init`] instead.
#[derive(Clone, Debug)]
pub struct RticAppShell {
    pub crate_imports: Vec<String>,
    pub device: String,
    pub app_module: String,
    pub dispatchers: Vec<String>,
    pub shared: String,
    pub local: String,
    pub init: String,
}

/// System/target-owned pieces surrounding a transplanted init package.
///
/// Unlike [`RticAppShell`], this does not accept source fragments for
/// `Shared`, `Local`, or `init`; those items come from [`InitPackage`].
#[derive(Clone, Debug)]
pub struct RticAppTarget {
    pub crate_imports: Vec<String>,
    pub device: String,
    pub app_module: String,
    pub dispatchers: Vec<String>,
}

/// Backward-compatible name for the first single-module proof.
pub type SingleModuleRticShell = RticAppShell;

/// Stable generated namespace name for one logical source module.
pub fn module_namespace(module: &ModuleId) -> String {
    if module.path.is_empty() {
        return "__ferroforge_module_root".to_owned();
    }
    let mut name = "__ferroforge_module".to_owned();
    for segment in &module.path {
        let segment = identifier_key_str(segment);
        name.push('_');
        name.push_str(&segment.len().to_string());
        name.push('_');
        name.push_str(&segment);
    }
    name
}

/// Retain the original single-module gate for callers that need that guarantee.
pub fn render_single_module_rtic_app(
    sources: &TaskSources,
    composition: &ValidatedComposition<'_>,
    shell: &SingleModuleRticShell,
) -> Result<String, RenderError> {
    let modules = composition
        .tasks
        .iter()
        .map(|task| &task.source.definition.id.module)
        .collect::<BTreeSet<_>>();
    if modules.len() != 1 {
        return Err(invalid(
            "early standalone RTIC layout supports one logical source module",
        ));
    }
    render_rtic_app(sources, composition, shell)
}

/// Render selected logical source modules into isolated namespaces around one
/// real RTIC app.
///
/// Every handler imports only its own support namespace, so ordinary colliding
/// names from different source modules remain separate. Resource renaming is
/// syntax-aware for `context.local.FIELD` and `context.shared.FIELD`.
/// Direct configuration field references are translated to per-instance typed
/// constants, typed spawn aliases are translated to selected RTIC task
/// instances, and the initial SysTick delay profile is bound to a real
/// `rtic-monotonics` declaration.
pub fn render_rtic_app(
    sources: &TaskSources,
    composition: &ValidatedComposition<'_>,
    shell: &RticAppShell,
) -> Result<String, RenderError> {
    let target = RticAppTarget {
        crate_imports: shell.crate_imports.clone(),
        device: shell.device.clone(),
        app_module: shell.app_module.clone(),
        dispatchers: shell.dispatchers.clone(),
    };
    let mut shared: ItemStruct = syn::parse_str(&shell.shared)?;
    shared.attrs.insert(0, parse_quote!(#[shared]));
    let mut local: ItemStruct = syn::parse_str(&shell.local)?;
    local.attrs.insert(0, parse_quote!(#[local]));
    let mut init: ItemFn = syn::parse_str(&shell.init)?;
    init.attrs.insert(0, parse_quote!(#[init]));
    render_rtic_app_parts(
        sources,
        composition,
        &target,
        TransplantedInit {
            attributes: Vec::new(),
            support: Vec::new(),
            shared,
            local,
            init,
        },
    )
}

/// Render selected task source together with an independently checked native
/// init package into one real RTIC app.
///
/// The initial integration accepts the same qualified crate-root init and
/// SysTick profile as the independent checker. `Shared`, `Local`, ordinary root
/// support, and the complete init body come from authored source rather than
/// shell fragments.
pub fn render_rtic_app_with_init(
    sources: &TaskSources,
    init_package: &InitPackage,
    composition: &ValidatedComposition<'_>,
    target: &RticAppTarget,
) -> Result<String, RenderError> {
    let init = transplant_init(init_package, composition)?;
    render_rtic_app_parts(sources, composition, target, init)
}

struct TransplantedInit {
    attributes: Vec<Attribute>,
    support: Vec<Item>,
    shared: ItemStruct,
    local: ItemStruct,
    init: ItemFn,
}

fn render_rtic_app_parts(
    sources: &TaskSources,
    composition: &ValidatedComposition<'_>,
    target: &RticAppTarget,
    init_items: TransplantedInit,
) -> Result<String, RenderError> {
    if composition.tasks.is_empty() {
        return Err(invalid("standalone RTIC layout needs at least one task"));
    }
    let module_ids = composition
        .tasks
        .iter()
        .map(|task| task.source.definition.id.module.clone())
        .collect::<BTreeSet<_>>();
    for left in &module_ids {
        for right in &module_ids {
            if left != right
                && (left.path.starts_with(&right.path) || right.path.starts_with(&left.path))
            {
                return Err(invalid(
                    "early standalone RTIC layout does not support selecting both an ancestor module and its descendant",
                ));
            }
        }
    }
    let namespaces = module_ids
        .iter()
        .map(|id| {
            let namespace: Ident = syn::parse_str(&module_namespace(id))?;
            Ok((id.clone(), namespace))
        })
        .collect::<Result<BTreeMap<_, _>, RenderError>>()?;

    let crate_imports = target
        .crate_imports
        .iter()
        .map(|source| {
            let item: Item = syn::parse_str(source)?;
            if !matches!(item, Item::Use(_) | Item::ExternCrate(_)) {
                return Err(invalid("RTIC shell crate imports must be use items"));
            }
            Ok(item)
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    let device: Path = syn::parse_str(&target.device)?;
    let app_module: Ident = syn::parse_str(&target.app_module)
        .map_err(|_| invalid(format!("invalid RTIC app module `{}`", target.app_module)))?;
    let dispatchers = target
        .dispatchers
        .iter()
        .map(|name| {
            syn::parse_str::<Ident>(name)
                .map_err(|_| invalid(format!("invalid dispatcher `{name}`")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let TransplantedInit {
        attributes: init_attributes,
        support: init_support,
        shared,
        local,
        init,
    } = init_items;

    let support_modules = namespaces
        .iter()
        .map(|(id, namespace)| render_source_namespace(sources, id, namespace))
        .collect::<Result<Vec<_>, _>>()?;
    let handlers = composition
        .tasks
        .iter()
        .map(|task| {
            let namespace = namespaces
                .get(&task.source.definition.id.module)
                .expect("selected module namespace was created");
            render_handler(task, namespace)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let configuration = render_configuration_constants(composition)?;
    let monotonic = render_monotonic(composition)?;
    let monotonic_import = composition.monotonic.as_ref().map(|_| {
        quote!(
            use rtic_monotonics::systick::prelude::*;
        )
    });

    let file: File = syn::parse2(quote! {
        #![no_std]
        #![no_main]

        #(#crate_imports)*

        #monotonic

        mod __ferroforge_sources {
            #(#support_modules)*

            #[rtic::app(device = #device, dispatchers = [#(#dispatchers),*])]
            mod #app_module {
                #(#init_attributes)*
                use super::*;
                #monotonic_import
                #(#init_support)*

                #configuration
                #shared
                #local
                #init
                #(#handlers)*
            }
        }
    })?;
    Ok(prettyplease::unparse(&file))
}

fn transplant_init(
    package: &InitPackage,
    composition: &ValidatedComposition<'_>,
) -> Result<TransplantedInit, RenderError> {
    if composition.monotonic.as_ref() != Some(&MonotonicProfile::initial_systick()) {
        return Err(invalid(
            "initial init transplantation requires the SysTick profile at 1000 Hz with u32 time values",
        ));
    }
    if package.init.module != package.package.sources.root
        || !package.init.module.path.is_empty()
        || !package.init.function.attrs.iter().any(is_qualified_init)
    {
        return Err(invalid(
            "initial init transplantation requires a qualified crate-root `#[ferroforge::init]` declaration",
        ));
    }

    let module = package
        .package
        .sources
        .modules
        .get(&package.init.module)
        .ok_or_else(|| invalid("init source module is absent from the supplied init package"))?;
    if !module.children.is_empty() {
        return Err(invalid(
            "initial init transplantation requires self-contained crate-root support without child modules",
        ));
    }
    let (outer_attributes, attributes) = split_attributes(&module.attributes);
    if !outer_attributes.is_empty() {
        return Err(invalid(format!(
            "{}: outer crate attributes are not supported by initial init transplantation",
            module.file.display()
        )));
    }

    let mut shared = None;
    let mut local = None;
    let mut support = Vec::new();
    for item in module.support.iter().cloned() {
        match item {
            Item::Struct(item) if item.ident == "Shared" => {
                if shared.replace(item).is_some() {
                    return Err(invalid(format!(
                        "{}: init support defines `Shared` more than once",
                        module.file.display()
                    )));
                }
            }
            Item::Struct(item) if item.ident == "Local" => {
                if local.replace(item).is_some() {
                    return Err(invalid(format!(
                        "{}: init support defines `Local` more than once",
                        module.file.display()
                    )));
                }
            }
            item if matches!(&item, Item::Use(item) if use_root(&item.tree).is_some_and(|root| root == "ferroforge"))
                || matches!(&item, Item::ExternCrate(item) if item.ident == "ferroforge") => {}
            item => support.push(item),
        }
    }
    let mut shared = shared.ok_or_else(|| {
        invalid(format!(
            "{}: init support must define `Shared` at the crate root",
            module.file.display()
        ))
    })?;
    let mut local = local.ok_or_else(|| {
        invalid(format!(
            "{}: init support must define `Local` at the crate root",
            module.file.display()
        ))
    })?;
    shared.attrs.insert(0, parse_quote!(#[shared]));
    local.attrs.insert(0, parse_quote!(#[local]));

    let mut init = package.init.function.clone();
    init.attrs.retain(|attribute| !is_qualified_init(attribute));
    init.attrs.insert(0, parse_quote!(#[init]));
    let mut monotonic = InitMonotonicRewriter::default();
    monotonic.visit_item_fn_mut(&mut init);
    monotonic.finish(&module.file)?;

    let retained = quote!(#(#support)* #shared #local #init);
    if token_stream_contains_ident(&retained, "ferroforge") {
        return Err(invalid(format!(
            "{}: unsupported checking-only `ferroforge` reference remains in transplanted init source",
            module.file.display()
        )));
    }

    Ok(TransplantedInit {
        attributes,
        support,
        shared,
        local,
        init,
    })
}

#[derive(Default)]
struct InitMonotonicRewriter {
    error: Option<String>,
}

impl InitMonotonicRewriter {
    fn finish(self, file: &std::path::Path) -> Result<(), RenderError> {
        match self.error {
            Some(error) => Err(invalid(format!("{}: {error}", file.display()))),
            None => Ok(()),
        }
    }

    fn reject(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
    }

    fn starts_with_mono(path: &Path) -> bool {
        path.leading_colon.is_none()
            && path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "Mono")
    }
}

impl VisitMut for InitMonotonicRewriter {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if self.error.is_some() {
            return;
        }
        if let Expr::Call(call) = expression
            && let Expr::Path(function) = call.func.as_ref()
            && Self::starts_with_mono(&function.path)
        {
            let is_start = function.qself.is_none()
                && function.path.segments.len() == 2
                && function.path.segments[1].ident == "start";
            if !is_start {
                self.reject(
                    "init uses an unsupported monotonic operation; the initial transplant supports only direct `Mono::start` calls",
                );
                return;
            }
            if call.args.len() != 2 {
                self.reject("init monotonic `Mono::start` call must have two arguments");
                return;
            }
            for argument in &mut call.args {
                self.visit_expr_mut(argument);
            }
            if self.error.is_some() {
                return;
            }
            let attributes = call.attrs.clone();
            let arguments = &call.args;
            let generated = generated_monotonic();
            let mut rewritten: Expr = parse_quote!(crate::#generated::start(#arguments));
            if let Expr::Call(call) = &mut rewritten {
                call.attrs = attributes;
            }
            *expression = rewritten;
            return;
        }
        if let Expr::Path(path) = expression
            && Self::starts_with_mono(&path.path)
        {
            self.reject("init uses `Mono` outside a direct `Mono::start` call");
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_macro_mut(&mut self, expression: &mut syn::Macro) {
        if token_stream_contains_ident(&expression.tokens, "Mono") {
            self.reject(
                "init references `Mono` inside a macro; macro token rewriting is not supported",
            );
        }
    }
}

fn render_monotonic(composition: &ValidatedComposition<'_>) -> Result<TokenStream, RenderError> {
    let Some(profile) = composition.monotonic.as_ref() else {
        return Ok(TokenStream::new());
    };
    if profile.source != MonotonicSource::SysTick
        || profile.tick_hz != INITIAL_SYSTICK_TICK_HZ
        || profile.counter_bits != INITIAL_SYSTICK_COUNTER_BITS
    {
        return Err(invalid(format!(
            "early standalone RTIC layout requires SysTick at {} Hz with u{} time values",
            INITIAL_SYSTICK_TICK_HZ, INITIAL_SYSTICK_COUNTER_BITS
        )));
    }
    let monotonic = generated_monotonic();
    let tick_hz = profile.tick_hz;
    Ok(quote! {
        use rtic_monotonics::systick::prelude::*;
        systick_monotonic!(#monotonic, #tick_hz);
    })
}

fn generated_monotonic() -> Ident {
    parse_quote!(__FerroforgeMono)
}

fn render_configuration_constants(
    composition: &ValidatedComposition<'_>,
) -> Result<TokenStream, RenderError> {
    let groups = composition
        .tasks
        .iter()
        .filter(|task| !task.configuration.is_empty())
        .map(|task| {
            let instance: Ident = syn::parse_str(&task.source.name).map_err(|_| {
                invalid(format!(
                    "cannot generate configuration group for `{}`",
                    task.source.name
                ))
            })?;
            let constants = task
                .configuration
                .iter()
                .map(|binding| {
                    let name = configuration_field(&binding.name)?;
                    let ty: Type = syn::parse_str(&binding.rust_type)?;
                    let value: Expr = syn::parse_str(&binding.value)?;
                    Ok::<_, RenderError>(quote! {
                        pub(crate) const #name: #ty = #value;
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            Ok::<_, RenderError>(quote! {
                pub(super) mod #instance {
                    #(#constants)*
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if groups.is_empty() {
        Ok(TokenStream::new())
    } else {
        Ok(quote! {
            mod __ferroforge_config {
                #(#groups)*
            }
        })
    }
}

fn configuration_field(field: &str) -> Result<Ident, RenderError> {
    let field = identifier_key_str(field).to_shouty_snake_case();
    syn::parse_str(&field)
        .map_err(|_| invalid(format!("cannot generate configuration field `{field}`")))
}

fn configuration_path(instance: &str, field: &str) -> Result<Path, RenderError> {
    let instance: Ident = syn::parse_str(instance).map_err(|_| {
        invalid(format!(
            "cannot generate configuration group for `{instance}`"
        ))
    })?;
    let field = configuration_field(field)?;
    Ok(parse_quote!(__ferroforge_config::#instance::#field))
}

fn render_source_namespace(
    sources: &TaskSources,
    id: &ModuleId,
    namespace: &Ident,
) -> Result<TokenStream, RenderError> {
    let module = sources.modules.get(id).ok_or_else(|| {
        invalid("selected task source module is absent from the supplied source set")
    })?;
    let (outer_attributes, inner_attributes) = split_attributes(&module.attributes);
    let contents = render_support_contents(sources, module, id, namespace)?;
    Ok(quote! {
        #(#outer_attributes)*
        pub mod #namespace {
            #(#inner_attributes)*
            #contents
        }
    })
}

fn render_support_contents(
    sources: &TaskSources,
    module: &ModuleSource,
    selected_root: &ModuleId,
    namespace: &Ident,
) -> Result<TokenStream, RenderError> {
    let support = module
        .support
        .iter()
        .cloned()
        .map(|item| prepare_support_item(item, module, selected_root, namespace))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let children = module
        .children
        .iter()
        .map(|id| render_child_module(sources, id, selected_root, namespace))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(quote! {
        #(#support)*
        #(#children)*
    })
}

fn render_child_module(
    sources: &TaskSources,
    id: &ModuleId,
    selected_root: &ModuleId,
    namespace: &Ident,
) -> Result<TokenStream, RenderError> {
    let module = sources
        .modules
        .get(id)
        .ok_or_else(|| invalid("discovered child module is absent from the source set"))?;
    let name: Ident = syn::parse_str(
        id.path
            .last()
            .ok_or_else(|| invalid("child module has no name"))?,
    )?;
    let (outer_attributes, inner_attributes) = split_attributes(&module.attributes);
    let contents = render_support_contents(sources, module, selected_root, namespace)?;
    Ok(quote! {
        #(#outer_attributes)*
        pub mod #name {
            #(#inner_attributes)*
            #contents
        }
    })
}

fn split_attributes(attributes: &[Attribute]) -> (Vec<Attribute>, Vec<Attribute>) {
    let mut outer = Vec::new();
    let mut inner = Vec::new();
    for attribute in attributes {
        if attribute.path().is_ident("no_std") {
            continue;
        }
        match attribute.style {
            AttrStyle::Outer => outer.push(attribute.clone()),
            AttrStyle::Inner(_) => inner.push(attribute.clone()),
        }
    }
    (outer, inner)
}

fn prepare_support_item(
    mut item: Item,
    module: &ModuleSource,
    selected_root: &ModuleId,
    namespace: &Ident,
) -> Result<Option<Item>, RenderError> {
    if matches!(&item, Item::Use(item) if use_root(&item.tree).is_some_and(|root| root == "ferroforge"))
        || matches!(&item, Item::ExternCrate(item) if item.ident == "ferroforge")
    {
        return Ok(None);
    }
    let relative_depth = module.id.path.len() - selected_root.path.len();
    let mut references = ReferenceRewriter::support(selected_root, namespace, relative_depth);
    references.visit_item_mut(&mut item);
    references.finish(&module.file)?;
    let public: Visibility = parse_quote!(pub);
    match &mut item {
        Item::Const(item) => item.vis = public,
        Item::Enum(item) => item.vis = public,
        Item::Fn(item) => item.vis = public,
        Item::Impl(item) if item.trait_.is_none() => {
            for impl_item in &mut item.items {
                match impl_item {
                    syn::ImplItem::Const(item) => item.vis = public.clone(),
                    syn::ImplItem::Fn(item) => item.vis = public.clone(),
                    syn::ImplItem::Type(item) => item.vis = public.clone(),
                    _ => {}
                }
            }
        }
        Item::Static(item) => item.vis = public,
        Item::Struct(item) => {
            item.vis = public.clone();
            for field in &mut item.fields {
                field.vis = public.clone();
            }
        }
        Item::Trait(item) => item.vis = public,
        Item::TraitAlias(item) => item.vis = public,
        Item::Type(item) => item.vis = public,
        Item::Union(item) => {
            item.vis = public.clone();
            for field in &mut item.fields.named {
                field.vis = public.clone();
            }
        }
        Item::Use(item) => item.vis = public,
        _ => {}
    }
    Ok(Some(item))
}

fn use_root(tree: &syn::UseTree) -> Option<String> {
    match tree {
        syn::UseTree::Path(path) => Some(identifier_key(&path.ident)),
        syn::UseTree::Name(name) => Some(identifier_key(&name.ident)),
        syn::UseTree::Rename(rename) => Some(identifier_key(&rename.ident)),
        syn::UseTree::Glob(_) | syn::UseTree::Group(_) => None,
    }
}

#[derive(Clone, Copy)]
enum ReferenceMode {
    Task,
    Support { relative_depth: usize },
}

struct ReferenceRewriter<'a> {
    selected_root: &'a ModuleId,
    namespace: &'a Ident,
    mode: ReferenceMode,
    error: Option<String>,
}

impl<'a> ReferenceRewriter<'a> {
    fn task(selected_root: &'a ModuleId, namespace: &'a Ident) -> Self {
        Self {
            selected_root,
            namespace,
            mode: ReferenceMode::Task,
            error: None,
        }
    }

    fn support(selected_root: &'a ModuleId, namespace: &'a Ident, relative_depth: usize) -> Self {
        Self {
            selected_root,
            namespace,
            mode: ReferenceMode::Support { relative_depth },
            error: None,
        }
    }

    fn finish(self, file: &std::path::Path) -> Result<(), RenderError> {
        match self.error {
            Some(error) => Err(invalid(format!("{}: {error}", file.display()))),
            None => Ok(()),
        }
    }

    fn generated_prefix(&self) -> Path {
        let namespace = self.namespace;
        parse_quote!(crate::__ferroforge_sources::#namespace)
    }

    fn rewrite_self(&self, path: &mut Path) {
        let remainder = path.segments.iter().skip(1).cloned().collect::<Vec<_>>();
        let mut rewritten = self.generated_prefix();
        rewritten.segments.extend(remainder);
        *path = rewritten;
    }

    fn rewrite_crate(&mut self, path: &mut Path) {
        let segments = path.segments.iter().cloned().collect::<Vec<_>>();
        let source_prefix = &self.selected_root.path;
        if segments.len() < 1 + source_prefix.len()
            || segments
                .iter()
                .skip(1)
                .zip(source_prefix)
                .any(|(actual, expected)| {
                    identifier_key(&actual.ident) != identifier_key_str(expected)
                })
        {
            self.error.get_or_insert_with(|| {
                format!(
                    "`crate::` reference escapes selected source module `{}`",
                    display_module(self.selected_root)
                )
            });
            return;
        }
        let remainder = segments.into_iter().skip(1 + source_prefix.len());
        let mut rewritten = self.generated_prefix();
        rewritten.segments.extend(remainder);
        *path = rewritten;
    }
}

impl VisitMut for ReferenceRewriter<'_> {
    fn visit_path_mut(&mut self, path: &mut Path) {
        if self.error.is_some() || path.leading_colon.is_some() || path.segments.is_empty() {
            return;
        }
        let first = identifier_key(&path.segments[0].ident);
        match first.as_str() {
            "self" if matches!(self.mode, ReferenceMode::Task) => self.rewrite_self(path),
            "super" => {
                let count = path
                    .segments
                    .iter()
                    .take_while(|segment| segment.ident == "super")
                    .count();
                let supported = matches!(
                    self.mode,
                    ReferenceMode::Support { relative_depth } if count <= relative_depth
                );
                if !supported {
                    self.error.get_or_insert_with(|| {
                        "`super::` reference escapes the selected source boundary".to_owned()
                    });
                }
            }
            "crate" => self.rewrite_crate(path),
            _ => {}
        }
        syn::visit_mut::visit_path_mut(self, path);
    }

    fn visit_item_use_mut(&mut self, item: &mut syn::ItemUse) {
        if use_root(&item.tree)
            .is_some_and(|root| matches!(root.as_str(), "self" | "super" | "crate"))
        {
            self.error.get_or_insert_with(|| {
                "relative `use` declarations are not supported by early transplantation".to_owned()
            });
            return;
        }
        syn::visit_mut::visit_item_use_mut(self, item);
    }
}

fn display_module(module: &ModuleId) -> String {
    if module.path.is_empty() {
        "crate root".to_owned()
    } else {
        module.path.join("::")
    }
}

fn render_handler(
    task: &ValidatedTask<'_>,
    support_namespace: &Ident,
) -> Result<TokenStream, RenderError> {
    let mut function = task.source.definition.function.clone();
    function.attrs.retain(|attribute| !is_task(attribute));
    function.vis = Visibility::Inherited;

    let instance: Ident = syn::parse_str(&task.source.name)
        .map_err(|_| invalid(format!("invalid task instance `{}`", task.source.name)))?;
    function.sig.ident = instance.clone();
    let context = function
        .sig
        .inputs
        .first_mut()
        .ok_or_else(|| invalid("task has no context parameter after validation"))?;
    let FnArg::Typed(context) = context else {
        return Err(invalid("task context cannot be a receiver"));
    };
    let Type::Path(context_type) = &mut *context.ty else {
        return Err(invalid("task context type is not a path"));
    };
    context_type.path.segments[0].ident = instance;

    let local = resource_map(&task.local, "local")?;
    let shared = resource_map(&task.shared, "shared")?;
    let mut logging = LoggingMacroRewriter::new(task, support_namespace, &local, &shared);
    logging.visit_block_mut(&mut function.block);
    logging.finish()?;

    let mut references =
        ReferenceRewriter::task(&task.source.definition.id.module, support_namespace);
    references.visit_block_mut(&mut function.block);
    references.finish(&task.source.support_module.file)?;

    let mut configuration = ConfigurationRewriter::new(task)?;
    configuration.visit_block_mut(&mut function.block);
    configuration.finish(&task.source.support_module.file)?;

    let mut spawn = SpawnRewriter::new(task)?;
    spawn.visit_block_mut(&mut function.block);
    spawn.finish(&task.source.support_module.file)?;

    let mut monotonic = MonotonicRewriter::new(task)?;
    monotonic.visit_block_mut(&mut function.block);
    monotonic.finish(&task.source.support_module.file)?;

    let mut rewriter = ResourceAccessRewriter {
        context: task.source.definition.contract.context.clone(),
        local: &local,
        shared: &shared,
        shadowed_context: false,
    };
    rewriter.visit_block_mut(&mut function.block);
    if rewriter.shadowed_context {
        return Err(invalid(format!(
            "task instance `{}` shadows its context parameter `{}`; resource rewriting would be ambiguous",
            task.source.name, task.source.definition.contract.context
        )));
    }
    let support_import: syn::Stmt = parse_quote! {
        #[allow(unused_imports)]
        use crate::__ferroforge_sources::#support_namespace::*;
    };
    function.block.stmts.insert(0, support_import);
    let local = local.values().cloned().collect::<Vec<_>>();
    let shared = shared.values().cloned().collect::<Vec<_>>();
    let priority = syn::LitInt::new(
        &task.priority.to_string(),
        task.source.definition.function.span(),
    );
    let attribute = match (local.is_empty(), shared.is_empty()) {
        (true, true) => quote!(#[task(priority = #priority)]),
        (false, true) => quote!(#[task(priority = #priority, local = [#(#local),*])]),
        (true, false) => quote!(#[task(priority = #priority, shared = [#(#shared),*])]),
        (false, false) => quote! {
            #[task(priority = #priority, local = [#(#local),*], shared = [#(#shared),*])]
        },
    };
    Ok(quote! {
        #attribute
        #function
    })
}

const DEFMT_LOGGING_MACROS: &[&str] = &["trace", "debug", "info", "warn", "error", "println"];
const RTT_LOGGING_MACROS: &[&str] = &["rprint", "rprintln"];

struct LoggingMacroRewriter<'a> {
    task: &'a ValidatedTask<'a>,
    support_namespace: &'a Ident,
    local: &'a BTreeMap<String, Ident>,
    shared: &'a BTreeMap<String, Ident>,
    known: BTreeSet<String>,
    error: Option<RenderError>,
}

impl<'a> LoggingMacroRewriter<'a> {
    fn new(
        task: &'a ValidatedTask<'a>,
        support_namespace: &'a Ident,
        local: &'a BTreeMap<String, Ident>,
        shared: &'a BTreeMap<String, Ident>,
    ) -> Self {
        Self {
            task,
            support_namespace,
            local,
            shared,
            known: logging_macro_paths(task.source.support_module),
            error: None,
        }
    }

    fn finish(self) -> Result<(), RenderError> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn rewrite_expression(&self, expression: &mut Expr) -> Result<(), RenderError> {
        let file = &self.task.source.support_module.file;
        let mut references = ReferenceRewriter::task(
            &self.task.source.definition.id.module,
            self.support_namespace,
        );
        references.visit_expr_mut(expression);
        references.finish(file)?;

        let mut configuration = ConfigurationRewriter::new(self.task)?;
        configuration.visit_expr_mut(expression);
        configuration.finish(file)?;

        let mut spawn = SpawnRewriter::new(self.task)?;
        spawn.visit_expr_mut(expression);
        spawn.finish(file)?;

        let mut monotonic = MonotonicRewriter::new(self.task)?;
        monotonic.visit_expr_mut(expression);
        monotonic.finish(file)?;

        let mut resources = ResourceAccessRewriter {
            context: self.task.source.definition.contract.context.clone(),
            local: self.local,
            shared: self.shared,
            shadowed_context: false,
        };
        resources.visit_expr_mut(expression);
        if resources.shadowed_context {
            return Err(invalid(format!(
                "{}: task instance `{}` shadows its context parameter `{}` inside a logging argument",
                file.display(),
                self.task.source.name,
                self.task.source.definition.contract.context
            )));
        }
        Ok(())
    }
}

impl VisitMut for LoggingMacroRewriter<'_> {
    fn visit_macro_mut(&mut self, invocation: &mut syn::Macro) {
        if self.error.is_some() || !self.known.contains(&path_key(&invocation.path)) {
            return;
        }
        let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
        let mut arguments = match parser.parse2(invocation.tokens.clone()) {
            Ok(arguments) => arguments,
            Err(error) => {
                self.error = Some(invalid(format!(
                    "{}: cannot parse supported logging macro `{}` arguments: {error}",
                    self.task.source.support_module.file.display(),
                    path_key(&invocation.path)
                )));
                return;
            }
        };
        for argument in &mut arguments {
            self.visit_expr_mut(argument);
            if self.error.is_some() {
                return;
            }
            if let Err(error) = self.rewrite_expression(argument) {
                self.error = Some(error);
                return;
            }
        }
        invocation.tokens = quote!(#arguments);
    }
}

fn logging_macro_paths(module: &ModuleSource) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for name in DEFMT_LOGGING_MACROS {
        paths.insert(format!("defmt::{name}"));
    }
    for name in RTT_LOGGING_MACROS {
        paths.insert(format!("rtt_target::{name}"));
    }

    let mut imports = Vec::new();
    for item in &module.support {
        if let Item::Use(item) = item {
            flatten_use(&item.tree, &mut Vec::new(), &mut imports);
        }
    }
    for (canonical, local, glob) in imports {
        let Some(provider) = canonical.first().map(String::as_str) else {
            continue;
        };
        let macros = match provider {
            "defmt" => DEFMT_LOGGING_MACROS,
            "rtt_target" => RTT_LOGGING_MACROS,
            _ => continue,
        };
        match canonical.as_slice() {
            [_provider] if !glob => {
                for name in macros {
                    paths.insert(format!("{local}::{name}"));
                }
            }
            [_provider] if glob => {
                for name in macros {
                    paths.insert((*name).to_owned());
                }
            }
            [_provider, name] if macros.contains(&name.as_str()) => {
                paths.insert(local);
            }
            _ => {}
        }
    }
    paths
}

fn flatten_use(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    output: &mut Vec<(Vec<String>, String, bool)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(identifier_key(&path.ident));
            flatten_use(&path.tree, prefix, output);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let name = identifier_key(&name.ident);
            if name == "self" {
                if let Some(local) = prefix.last() {
                    output.push((prefix.clone(), local.clone(), false));
                }
            } else {
                let mut canonical = prefix.clone();
                canonical.push(name.clone());
                output.push((canonical, name, false));
            }
        }
        syn::UseTree::Rename(rename) => {
            let source = identifier_key(&rename.ident);
            let mut canonical = prefix.clone();
            if source != "self" {
                canonical.push(source);
            }
            output.push((canonical, identifier_key(&rename.rename), false));
        }
        syn::UseTree::Glob(_) => {
            output.push((prefix.clone(), "*".to_owned(), true));
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                flatten_use(item, prefix, output);
            }
        }
    }
}

fn path_key(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| identifier_key(&segment.ident))
        .collect::<Vec<_>>()
        .join("::")
}

struct ConfigurationRewriter {
    instance: String,
    fields: BTreeMap<String, Path>,
    error: Option<String>,
}

impl ConfigurationRewriter {
    fn new(task: &ValidatedTask<'_>) -> Result<Self, RenderError> {
        let fields = task
            .configuration
            .iter()
            .map(|binding| {
                let source_name = identifier_key_str(&binding.name).to_shouty_snake_case();
                let generated = configuration_path(&task.source.name, &binding.name)?;
                Ok((source_name, generated))
            })
            .collect::<Result<_, RenderError>>()?;
        Ok(Self {
            instance: task.source.name.clone(),
            fields,
            error: None,
        })
    }

    fn finish(self, file: &std::path::Path) -> Result<(), RenderError> {
        match self.error {
            Some(error) => Err(invalid(format!("{}: {error}", file.display()))),
            None => Ok(()),
        }
    }

    fn reject(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
    }
}

impl VisitMut for ConfigurationRewriter {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if self.error.is_some() {
            return;
        }
        if let Expr::Field(field) = expression
            && let Expr::Path(base) = field.base.as_ref()
            && base.qself.is_none()
            && base.path.is_ident("CONFIG")
        {
            let Member::Named(member) = &field.member else {
                self.reject(format!(
                    "task instance `{}` uses an unnamed `CONFIG` field",
                    self.instance
                ));
                return;
            };
            let source_name = identifier_key(member);
            let Some(generated) = self.fields.get(&source_name) else {
                self.reject(format!(
                    "task instance `{}` references unknown configuration field `CONFIG.{source_name}`",
                    self.instance
                ));
                return;
            };
            *expression = parse_quote!(#generated);
            return;
        }
        if let Expr::Path(path) = expression
            && path.qself.is_none()
            && path.path.is_ident("CONFIG")
        {
            self.reject(format!(
                "task instance `{}` uses bare `CONFIG`; access a declared field directly",
                self.instance
            ));
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_macro_mut(&mut self, expression: &mut syn::Macro) {
        if token_stream_contains_ident(&expression.tokens, "CONFIG") {
            self.reject(format!(
                "task instance `{}` references `CONFIG` inside a macro; macro token rewriting is not supported",
                self.instance
            ));
        }
    }

    fn visit_pat_ident_mut(&mut self, pattern: &mut PatIdent) {
        if pattern.ident == "CONFIG" {
            self.reject(format!(
                "task instance `{}` shadows the reserved `CONFIG` name",
                self.instance
            ));
            return;
        }
        syn::visit_mut::visit_pat_ident_mut(self, pattern);
    }
}

fn token_stream_contains_ident(tokens: &TokenStream, expected: &str) -> bool {
    tokens.clone().into_iter().any(|token| match token {
        TokenTree::Ident(identifier) => identifier == expected,
        TokenTree::Group(group) => token_stream_contains_ident(&group.stream(), expected),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}

struct SpawnRewriter {
    instance: String,
    context: Ident,
    targets: BTreeMap<String, Ident>,
    error: Option<String>,
}

impl SpawnRewriter {
    fn new(task: &ValidatedTask<'_>) -> Result<Self, RenderError> {
        let targets = task
            .spawn
            .iter()
            .map(|binding| {
                let target: Ident = syn::parse_str(&binding.target)
                    .map_err(|_| invalid(format!("invalid spawn target `{}`", binding.target)))?;
                Ok((identifier_key_str(&binding.alias), target))
            })
            .collect::<Result<_, RenderError>>()?;
        Ok(Self {
            instance: task.source.name.clone(),
            context: task.source.definition.contract.context.clone(),
            targets,
            error: None,
        })
    }

    fn finish(self, file: &std::path::Path) -> Result<(), RenderError> {
        match self.error {
            Some(error) => Err(invalid(format!("{}: {error}", file.display()))),
            None => Ok(()),
        }
    }

    fn reject(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
    }

    fn is_spawn_handle(&self, expression: &Expr) -> bool {
        let Expr::Field(field) = expression else {
            return false;
        };
        let Member::Named(member) = &field.member else {
            return false;
        };
        let Expr::Path(context) = field.base.as_ref() else {
            return false;
        };
        identifier_key(member) == "spawn"
            && context.qself.is_none()
            && context.path.is_ident(&self.context)
    }
}

impl VisitMut for SpawnRewriter {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if self.error.is_some() {
            return;
        }
        if let Expr::MethodCall(call) = expression
            && self.is_spawn_handle(&call.receiver)
        {
            let alias = identifier_key(&call.method);
            let Some(target) = self.targets.get(&alias).cloned() else {
                self.reject(format!(
                    "task instance `{}` calls unknown spawn alias `{alias}`",
                    self.instance
                ));
                return;
            };
            if call.turbofish.is_some() {
                self.reject(format!(
                    "task instance `{}` uses generic arguments on spawn alias `{alias}`",
                    self.instance
                ));
                return;
            }
            for argument in &mut call.args {
                self.visit_expr_mut(argument);
            }
            if self.error.is_some() {
                return;
            }
            let attributes = call.attrs.clone();
            let arguments = &call.args;
            let mut rewritten: Expr = parse_quote!(#target::spawn(#arguments));
            if let Expr::Call(call) = &mut rewritten {
                call.attrs = attributes;
            }
            *expression = rewritten;
            return;
        }
        if self.is_spawn_handle(expression) {
            self.reject(format!(
                "task instance `{}` uses its spawn handle outside a direct alias call",
                self.instance
            ));
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_macro_mut(&mut self, expression: &mut syn::Macro) {
        if token_stream_contains_member(&expression.tokens, &self.context, "spawn") {
            self.reject(format!(
                "task instance `{}` references its spawn handle inside a macro; macro token rewriting is not supported",
                self.instance
            ));
        }
    }
}

fn token_stream_contains_member(tokens: &TokenStream, base: &Ident, member: &str) -> bool {
    let tokens = tokens.clone().into_iter().collect::<Vec<_>>();
    if tokens.windows(3).any(|window| {
        matches!(&window[0], TokenTree::Ident(identifier) if identifier == base)
            && matches!(&window[1], TokenTree::Punct(punctuation) if punctuation.as_char() == '.')
            && matches!(&window[2], TokenTree::Ident(identifier) if identifier == member)
    }) {
        return true;
    }
    tokens.into_iter().any(|token| match token {
        TokenTree::Group(group) => token_stream_contains_member(&group.stream(), base, member),
        TokenTree::Ident(_) | TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}

struct MonotonicRewriter {
    instance: String,
    source: Option<Ident>,
    error: Option<String>,
}

impl MonotonicRewriter {
    fn new(task: &ValidatedTask<'_>) -> Result<Self, RenderError> {
        let source = task
            .source
            .definition
            .contract
            .arguments
            .monotonic
            .as_ref()
            .map(|path| {
                path.get_ident().cloned().ok_or_else(|| {
                    invalid(format!(
                        "task instance `{}` uses a qualified monotonic source; the initial transplant supports one imported identifier",
                        task.source.name
                    ))
                })
            })
            .transpose()?;
        Ok(Self {
            instance: task.source.name.clone(),
            source,
            error: None,
        })
    }

    fn finish(self, file: &std::path::Path) -> Result<(), RenderError> {
        match self.error {
            Some(error) => Err(invalid(format!("{}: {error}", file.display()))),
            None => Ok(()),
        }
    }

    fn reject(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
    }

    fn starts_with_source(&self, path: &Path) -> bool {
        let Some(source) = &self.source else {
            return false;
        };
        path.leading_colon.is_none()
            && path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == *source)
    }
}

impl VisitMut for MonotonicRewriter {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if self.error.is_some() || self.source.is_none() {
            return;
        }
        if let Expr::Call(call) = expression
            && let Expr::Path(function) = call.func.as_ref()
            && self.starts_with_source(&function.path)
        {
            let is_delay = function.qself.is_none()
                && function.path.segments.len() == 2
                && function.path.segments[1].ident == "delay";
            if !is_delay {
                self.reject(format!(
                    "task instance `{}` uses an unsupported monotonic operation; the initial transplant supports only direct `delay` calls",
                    self.instance
                ));
                return;
            }
            if call.args.len() != 1 {
                self.reject(format!(
                    "task instance `{}` monotonic `delay` call must have one argument",
                    self.instance
                ));
                return;
            }
            for argument in &mut call.args {
                self.visit_expr_mut(argument);
            }
            if self.error.is_some() {
                return;
            }
            let attributes = call.attrs.clone();
            let arguments = &call.args;
            let generated = generated_monotonic();
            let mut rewritten: Expr = parse_quote!(crate::#generated::delay(#arguments));
            if let Expr::Call(call) = &mut rewritten {
                call.attrs = attributes;
            }
            *expression = rewritten;
            return;
        }
        if let Expr::Path(path) = expression
            && self.starts_with_source(&path.path)
        {
            self.reject(format!(
                "task instance `{}` uses its monotonic outside a direct `delay` call",
                self.instance
            ));
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_macro_mut(&mut self, expression: &mut syn::Macro) {
        let Some(source) = &self.source else {
            return;
        };
        if token_stream_contains_ident(&expression.tokens, &identifier_key(source)) {
            self.reject(format!(
                "task instance `{}` references its monotonic inside a macro; macro token rewriting is not supported",
                self.instance
            ));
        }
    }
}

fn resource_map(
    bindings: &[crate::composition::ResourceBinding],
    role: &str,
) -> Result<BTreeMap<String, Ident>, RenderError> {
    bindings
        .iter()
        .map(|binding| {
            let resource: Ident = syn::parse_str(&binding.resource).map_err(|_| {
                invalid(format!(
                    "invalid {role} system resource `{}`",
                    binding.resource
                ))
            })?;
            Ok((identifier_key_str(&binding.requirement), resource))
        })
        .collect()
}

struct ResourceAccessRewriter<'a> {
    context: Ident,
    local: &'a BTreeMap<String, Ident>,
    shared: &'a BTreeMap<String, Ident>,
    shadowed_context: bool,
}

impl VisitMut for ResourceAccessRewriter<'_> {
    fn visit_pat_ident_mut(&mut self, pattern: &mut PatIdent) {
        if identifier_key(&pattern.ident) == identifier_key(&self.context) {
            self.shadowed_context = true;
        }
        syn::visit_mut::visit_pat_ident_mut(self, pattern);
    }

    fn visit_expr_field_mut(&mut self, expression: &mut ExprField) {
        syn::visit_mut::visit_expr_field_mut(self, expression);
        let Member::Named(requirement) = &expression.member else {
            return;
        };
        let Expr::Field(category) = &*expression.base else {
            return;
        };
        let Member::Named(category_name) = &category.member else {
            return;
        };
        let Expr::Path(context) = &*category.base else {
            return;
        };
        if !context.path.is_ident(&self.context) {
            return;
        }
        let resources = if category_name == "local" {
            self.local
        } else if category_name == "shared" {
            self.shared
        } else {
            return;
        };
        if let Some(resource) = resources.get(&identifier_key(requirement)) {
            expression.member = Member::Named(resource.clone());
        }
    }
}

fn is_task(attribute: &Attribute) -> bool {
    let path = attribute.path();
    path.is_ident("task")
        || (path.segments.len() == 2
            && path.segments[0].ident == "ferroforge"
            && path.segments[1].ident == "task")
}

fn is_qualified_init(attribute: &Attribute) -> bool {
    let path = attribute.path();
    path.segments.len() == 2
        && path.segments[0].ident == "ferroforge"
        && path.segments[1].ident == "init"
}

fn identifier_key_str(identifier: &str) -> String {
    identifier
        .strip_prefix("r#")
        .unwrap_or(identifier)
        .to_owned()
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}
