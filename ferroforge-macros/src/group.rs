//! Task groups: tasks that only work as a set, selected by one declaration.
//!
//! A library marks a module of task definitions `#[ferroforge::group]`, as RTIC
//! marks its application `#[rtic::app] mod app`. The group reads each task's
//! signature and requirements from the definitions themselves. A module that
//! `pub use`s other groups' contents is the union of those groups.
//!
//! A firmware selects the whole set with one `#[group(from = ..)] mod name { .. }`
//! inside `app!`, binding the union of the group's resources once, and declares
//! each member as RTIC declares a task: `#[task(binds = USART1, priority = 12)]
//! fn on_uart;`.
//!
//! `app!` still reads nothing but its own input. `#[rtic::app]` must see every
//! task, so the members have to reach `app!` as tokens, and they come through a
//! callback: each group is also a `macro_rules!` holding its members, and
//! `app!`, finding a group it has no members for, hands the whole application
//! to that macro, which appends the members and calls `app!` again.
//!
//! ```text
//! app! { .. #[group(from = lib::serial::g)] mod x { .. } .. }
//!   -> lib::g! { x ; .. }                 (exported at the crate root)
//!   -> app! { .. @group x from g { use super::rx; members from self::.. } }
//!   -> lib::rx! { x ; .. }                (once per group g includes)
//!   -> #[rtic::app] mod app { .. x_member_a .. x_member_b .. mod x { .. } }
//! ```
//!
//! A group's macro cannot know where its module sits, so every path in what it
//! appends is relative - `self::on_uart`, `super::rx` - and `app!` makes it
//! absolute against the path the firmware wrote in `from`. A member's input
//! types travel as aliases in the group's module, so a type the library names
//! through its own imports is still nameable from the firmware.

use ferroforge_contracts::TaskArguments;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{
    Attribute, Ident, Item, ItemMod, ItemUse, LitInt, Path, Signature, Token, UseTree,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

use crate::app::{ConfigValue, Instance, LocalBinding, Rebind, TaskAttribute, bracketed_list};

impl ToTokens for Rebind {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let (requirement, target) = (&self.requirement, &self.target);
        tokens.extend(quote!(#requirement = #target));
    }
}

/// One member as its group states it.
///
/// ```rust,ignore
/// #[task(shared = [port], local = [uart], spawn = [frame = parse])]
/// fn on_uart();
/// ```
///
/// The signature is the definition's, past its context: `app!` needs it to
/// write the adapter and does not read the definition. Each list binds the
/// definition's requirement to a name in the group's namespace; a `spawn`
/// naming another member of the same group is wired inside the group.
#[derive(Clone)]
pub(crate) struct Member {
    docs: Vec<Attribute>,
    from: Option<Path>,
    shared: Vec<Rebind>,
    local: Vec<Rebind>,
    spawn: Vec<Rebind>,
    config: Vec<Rebind>,
    signature: Signature,
}

impl Parse for Member {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attributes = input.call(Attribute::parse_outer)?;
        let mut member = Self {
            docs: Vec::new(),
            from: None,
            shared: Vec::new(),
            local: Vec::new(),
            spawn: Vec::new(),
            config: Vec::new(),
            signature: parse_signature(input)?,
        };
        for attribute in attributes {
            if attribute.path().is_ident("doc") {
                member.docs.push(attribute);
                continue;
            }
            if !attribute.path().is_ident("task") {
                return Err(syn::Error::new(
                    attribute.span(),
                    "a group member takes `#[task(..)]` and documentation only",
                ));
            }
            attribute.parse_args_with(|input: ParseStream<'_>| {
                let mut seen: Vec<String> = Vec::new();
                while !input.is_empty() {
                    let key: Ident = input.parse()?;
                    if seen.contains(&key.to_string()) {
                        return Err(syn::Error::new(
                            key.span(),
                            format!("`{key}` appears more than once"),
                        ));
                    }
                    seen.push(key.to_string());
                    input.parse::<Token![=]>()?;
                    match key.to_string().as_str() {
                        "from" => member.from = Some(input.parse()?),
                        "shared" => member.shared = bracketed_list(input)?,
                        "local" => member.local = bracketed_list(input)?,
                        "spawn" => member.spawn = bracketed_list(input)?,
                        "config" => member.config = bracketed_list(input)?,
                        other => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!(
                                    "unknown member option `{other}`; a group binds \
                                     `from`, `shared`, `local`, `spawn` and `config`, \
                                     and the firmware sets priorities and interrupts"
                                ),
                            ));
                        }
                    }
                    if !input.is_empty() {
                        input.parse::<Token![,]>()?;
                    }
                }
                Ok(())
            })?;
        }
        Ok(member)
    }
}

fn parse_signature(input: ParseStream<'_>) -> syn::Result<Signature> {
    let signature: Signature = input.parse()?;
    input.parse::<Token![;]>()?;
    Ok(signature)
}

impl Member {
    fn name(&self) -> &Ident {
        &self.signature.ident
    }

    /// The member as a group's macro hands it to `app!`: the same form, with
    /// every path relative to the group's module as `self::..`, because only
    /// the firmware knows where that module is.
    fn emit(&self) -> TokenStream {
        let Self {
            docs,
            from,
            shared,
            local,
            spawn,
            config,
            signature,
        } = self;
        let from = match from {
            Some(path) => quote!(#path),
            None => {
                let name = &signature.ident;
                quote!(#name)
            }
        };
        quote! {
            #(#docs)*
            #[task(
                from = #from,
                shared = [#(#shared),*],
                local = [#(#local),*],
                spawn = [#(#spawn),*],
                config = [#(#config),*],
            )]
            #signature;
        }
    }
}

/// `#[ferroforge::group(spawn = [frame = parse])]`: which of the members'
/// outgoing calls the group wires to another member. Every other call is left
/// for the firmware to bind.
pub struct GroupArguments {
    spawn: Vec<Rebind>,
}

impl Parse for GroupArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut spawn = Vec::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "spawn" => spawn = bracketed_list(input)?,
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown group argument `{other}`; a group wires `spawn` \
                             between its members, and the firmware sets the rest"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self { spawn })
    }
}

/// `#[ferroforge::group]`: the module unchanged, plus an exported
/// `macro_rules!` of the same name holding its members.
///
/// The members are read from the definitions, so nothing is restated. A
/// definition's own locals, `sent: u32 = 0`, stay the definition's; only the
/// ones without a value are left for the firmware to bind. A `pub use` glob of
/// another group module, `pub use super::uart_dma_rx::*;`, includes that group.
pub fn define(arguments: GroupArguments, module: ItemMod) -> syn::Result<TokenStream> {
    let Some((_, items)) = &module.content else {
        return Err(syn::Error::new(
            module.span(),
            "a group is an inline module holding its task definitions",
        ));
    };
    let name = &module.ident;

    let mut members: Vec<Member> = Vec::new();
    let mut includes: Vec<Path> = Vec::new();
    // Every input type gets an alias in the group's module, where the author's
    // own imports resolve it, so the firmware can name it by path.
    let mut aliases: Vec<TokenStream> = Vec::new();
    for item in items {
        match item {
            Item::Fn(function) => {
                let Some(attribute) = function.attrs.iter().find(|attribute| {
                    attribute
                        .path()
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "task")
                }) else {
                    continue;
                };
                let task: TaskArguments = attribute.parse_args()?;
                let mut signature = function.sig.clone();
                let fn_name = &function.sig.ident;
                // The context is the adapter's to supply.
                signature.inputs = signature.inputs.into_iter().skip(1).collect();
                for (index, input) in signature.inputs.iter_mut().enumerate() {
                    if let syn::FnArg::Typed(typed) = input {
                        let alias = format_ident!("__ff_{}_input_{}", fn_name, index);
                        let ty = &typed.ty;
                        aliases.push(quote! {
                            #[doc(hidden)]
                            #[allow(non_camel_case_types)]
                            pub type #alias = #ty;
                        });
                        *typed.ty = syn::parse_quote!(self::#alias);
                    }
                }
                let bare = |names: Vec<&Ident>| {
                    names
                        .into_iter()
                        .map(|name| Rebind {
                            requirement: name.clone(),
                            target: name.clone(),
                        })
                        .collect::<Vec<_>>()
                };
                members.push(Member {
                    docs: Vec::new(),
                    from: Some(syn::parse_quote!(self::#fn_name)),
                    shared: bare(task.shared.iter().map(|r| &r.name).collect()),
                    local: bare(
                        task.local
                            .iter()
                            .filter(|r| r.init.is_none())
                            .map(|r| &r.name)
                            .collect(),
                    ),
                    spawn: task
                        .spawn
                        .iter()
                        .map(|call| Rebind {
                            requirement: call.name.clone(),
                            target: arguments
                                .spawn
                                .iter()
                                .find(|wire| wire.requirement == call.name)
                                .map_or_else(|| call.name.clone(), |wire| wire.target.clone()),
                        })
                        .collect(),
                    config: bare(task.config.iter().map(|r| &r.name).collect()),
                    signature,
                });
            }
            // Only a `pub use` re-exports another group's tasks as this
            // group's; a private `use super::*;` is an ordinary import.
            Item::Use(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                if let Some(group) = glob_source(&item.tree) {
                    includes.push(group);
                }
            }
            _ => {}
        }
    }

    for wire in &arguments.spawn {
        if !members.iter().any(|m| m.name() == &wire.target) {
            return Err(syn::Error::new(
                wire.target.span(),
                format!("`{}` is not a task in this group", wire.target),
            ));
        }
    }

    // A local belongs to one task. Two members of one group needing a local
    // of the same name would be bound to one firmware resource, which RTIC
    // refuses; say so where the group is written instead.
    for (index, member) in members.iter().enumerate() {
        for local in &member.local {
            if let Some(other) = members[index + 1..]
                .iter()
                .find(|other| other.local.iter().any(|l| l.target == local.target))
            {
                return Err(syn::Error::new(
                    name.span(),
                    format!(
                        "`{}` and `{}` both need a local called `{}`; rename one, \
                         because a local belongs to one task",
                        member.name(),
                        other.name(),
                        local.target
                    ),
                ));
            }
        }
    }

    let emitted = members.iter().map(Member::emit);
    let mut module = module.clone();
    if let Some((_, items)) = &mut module.content {
        for alias in aliases {
            items.push(syn::parse2(alias)?);
        }
    }
    Ok(quote! {
        #module

        #[doc(hidden)]
        #[macro_export]
        macro_rules! #name {
            ($instance:ident ; $($application:tt)*) => {
                ::ferroforge::app! {
                    $($application)*
                    @group $instance from #name {
                        #(use #includes;)*
                        #(#emitted)*
                    }
                }
            };
        }
    })
}

/// `use super::uart_dma_rx::*;` names the group at `super::uart_dma_rx`.
fn glob_source(tree: &UseTree) -> Option<Path> {
    let mut segments: Vec<Ident> = Vec::new();
    let mut tree = tree;
    while let UseTree::Path(path) = tree {
        segments.push(path.ident.clone());
        tree = &path.tree;
    }
    let is_glob = matches!(tree, UseTree::Glob(_));
    let names_a_module = segments
        .last()
        .is_some_and(|last| !["self", "super", "crate"].contains(&&*last.to_string()));
    (is_glob && names_a_module).then(|| syn::parse_quote!(#(#segments)::*))
}

/// What a group's macro appended: `@group <selection> { members }`.
pub(crate) struct GroupDefinition {
    pub(crate) instance: Ident,
    /// The group this came from, so a selection knows which it has.
    from: Path,
    /// Further groups this one is the union with, still to be asked.
    includes: Vec<Path>,
    members: Vec<Member>,
}

impl Parse for GroupDefinition {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![@]>()?;
        let keyword: Ident = input.parse()?;
        if keyword != "group" {
            return Err(syn::Error::new(keyword.span(), "expected `@group`"));
        }
        let instance = input.parse()?;
        let keyword: Ident = input.parse()?;
        if keyword != "from" {
            return Err(syn::Error::new(keyword.span(), "expected `from`"));
        }
        let from = input.parse()?;
        let content;
        syn::braced!(content in input);
        let mut includes = Vec::new();
        while content.peek(Token![use]) {
            content.parse::<Token![use]>()?;
            includes.push(content.parse()?);
            content.parse::<Token![;]>()?;
        }
        let mut members = Vec::new();
        while !content.is_empty() {
            members.push(content.parse()?);
        }
        Ok(Self {
            instance,
            from,
            includes,
            members,
        })
    }
}

/// One member as the firmware declares it, the way RTIC declares a task:
/// `#[task(binds = USART1, priority = 12)] fn on_uart;`. The signature is the
/// definition's, so it is not restated; `async` is, so the line reads as the
/// kind of task it is, and it is checked against the definition.
struct TaskSettings {
    attributes: Vec<Attribute>,
    asyncness: Option<Token![async]>,
    member: Ident,
    priority: Option<LitInt>,
    binds: Option<Path>,
}

impl Parse for TaskSettings {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let all = input.call(Attribute::parse_outer)?;
        let asyncness = input.parse()?;
        input.parse::<Token![fn]>()?;
        let member = input.parse()?;
        input.parse::<Token![;]>()?;
        let mut settings = Self {
            attributes: Vec::new(),
            asyncness,
            member,
            priority: None,
            binds: None,
        };
        for attribute in all {
            if !attribute.path().is_ident("task") {
                settings.attributes.push(attribute);
                continue;
            }
            attribute.parse_args_with(|content: ParseStream<'_>| {
                while !content.is_empty() {
                    let key: Ident = content.parse()?;
                    content.parse::<Token![=]>()?;
                    match key.to_string().as_str() {
                        "priority" => settings.priority = Some(content.parse()?),
                        "binds" => settings.binds = Some(content.parse()?),
                        other => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!(
                                    "unknown option `{other}`; a group member takes \
                                     `priority` and `binds`, and its resources are \
                                     bound once on the group"
                                ),
                            ));
                        }
                    }
                    if !content.is_empty() {
                        content.parse::<Token![,]>()?;
                    }
                }
                Ok(())
            })?;
        }
        Ok(settings)
    }
}

/// A firmware's selection of a whole group:
///
/// ```rust,ignore
/// #[group(
///     from = uart_dma::uart_dma,
///     shared = [port = uart],
///     local = [uart = usart, stream = tx_stream],
///     spawn = [decoded = sbus],
/// )]
/// mod sbus_link {
///     #[task(binds = USART1, priority = 12)]
///     fn on_uart;
///     #[task(priority = 1)]
///     async fn parse;
/// }
/// ```
pub(crate) struct GroupInstance {
    pub(crate) name: Ident,
    from: Path,
    shared: Vec<Rebind>,
    local: Vec<LocalBinding>,
    spawn: Vec<Rebind>,
    config: Vec<ConfigValue>,
    tasks: Vec<TaskSettings>,
    /// Documentation, `#[cfg]` and lints, given to every member's task.
    attributes: Vec<Attribute>,
}

impl GroupInstance {
    pub(crate) fn parse_with(
        input: ParseStream<'_>,
        attributes: Vec<Attribute>,
    ) -> syn::Result<Self> {
        let (group, attributes): (Vec<_>, Vec<_>) = attributes
            .into_iter()
            .partition(|attribute| attribute.path().is_ident("group"));
        let group = &group[0];

        let mut from = None;
        let mut shared = Vec::new();
        let mut local = Vec::new();
        let mut spawn = Vec::new();
        let mut config = Vec::new();
        group.parse_args_with(|input: ParseStream<'_>| {
            let mut seen: Vec<String> = Vec::new();
            while !input.is_empty() {
                let key: Ident = input.parse()?;
                if seen.contains(&key.to_string()) {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("`{key}` appears more than once"),
                    ));
                }
                seen.push(key.to_string());
                input.parse::<Token![=]>()?;
                match key.to_string().as_str() {
                    "from" => from = Some(input.parse()?),
                    "shared" => shared = bracketed_list(input)?,
                    "local" => local = bracketed_list(input)?,
                    "spawn" => spawn = bracketed_list(input)?,
                    "config" => config = bracketed_list(input)?,
                    other => {
                        return Err(syn::Error::new(
                            key.span(),
                            format!("unknown group option `{other}`"),
                        ));
                    }
                }
                if !input.is_empty() {
                    input.parse::<Token![,]>()?;
                }
            }
            Ok(())
        })?;
        let from: Path =
            from.ok_or_else(|| syn::Error::new(group.span(), "a group needs `from = <group>`"))?;

        input.parse::<Token![mod]>()?;
        let name = input.parse()?;
        let content;
        syn::braced!(content in input);
        let mut tasks = Vec::new();
        while !content.is_empty() {
            tasks.push(content.parse()?);
        }
        Ok(Self {
            name,
            from,
            shared,
            local,
            spawn,
            config,
            tasks,
            attributes,
        })
    }
}

/// The path of a `use`d name, so a group selected through an alias can be
/// called from where `app!` itself was invoked, outside the application's
/// imports.
fn resolve_alias(path: &Path, uses: &[&ItemUse]) -> Path {
    fn walk(tree: &UseTree, prefix: &mut Vec<Ident>, alias: &Ident) -> Option<Vec<Ident>> {
        match tree {
            UseTree::Path(inner) => {
                prefix.push(inner.ident.clone());
                let found = walk(&inner.tree, prefix, alias);
                prefix.pop();
                found
            }
            UseTree::Name(name) if &name.ident == alias => {
                let mut full = prefix.clone();
                full.push(name.ident.clone());
                Some(full)
            }
            UseTree::Rename(rename) if &rename.rename == alias => {
                let mut full = prefix.clone();
                if rename.ident != "self" {
                    full.push(rename.ident.clone());
                }
                Some(full)
            }
            UseTree::Group(group) => group
                .items
                .iter()
                .find_map(|tree| walk(tree, prefix, alias)),
            _ => None,
        }
    }

    if path.leading_colon.is_some() {
        return path.clone();
    }
    let first = &path.segments[0].ident;
    for item in uses {
        if let Some(full) = walk(&item.tree, &mut Vec::new(), first) {
            let rest = path.segments.iter().skip(1).map(|segment| &segment.ident);
            let leading = item.leading_colon;
            return syn::parse_quote!(#leading #(#full)::* #(:: #rest)*);
        }
    }
    path.clone()
}

fn last(path: &Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default()
}

/// This selection's definitions so far.
pub(crate) fn definitions_for<'a>(
    group: &GroupInstance,
    definitions: &[&'a GroupDefinition],
) -> Vec<&'a GroupDefinition> {
    definitions
        .iter()
        .filter(|definition| definition.instance == group.name)
        .copied()
        .collect()
}

/// `path` made absolute against the module `base`: `self::x` is inside it,
/// `super::x` beside it, `crate::x` at its crate's root. Anything else already
/// starts at a crate.
fn rebase(path: &Path, base: &Path) -> Path {
    let mut segments = path.segments.iter().peekable();
    let mut result: Vec<Ident> = base.segments.iter().map(|s| s.ident.clone()).collect();
    match segments.peek().map(|s| s.ident.to_string()) {
        Some(first) if first == "self" || first == "super" || first == "crate" => {}
        _ => return path.clone(),
    }
    while let Some(segment) = segments.peek() {
        match segment.ident.to_string().as_str() {
            "self" => {}
            "super" => {
                result.pop();
            }
            "crate" => result.truncate(1),
            _ => break,
        }
        segments.next();
    }
    result.extend(segments.map(|s| s.ident.clone()));
    let leading = base.leading_colon;
    syn::parse_quote!(#leading #(#result)::*)
}

/// Every group module this selection draws on, by name: the one it names and,
/// transitively, the ones those include. A module's path comes from the
/// firmware's own `from`, so a group may sit anywhere in its crate.
fn group_modules(
    group: &GroupInstance,
    mine: &[&GroupDefinition],
    uses: &[&ItemUse],
) -> Vec<(String, Path)> {
    let from = resolve_alias(&group.from, uses);
    let mut modules = vec![(last(&from), from)];
    let mut index = 0;
    while index < modules.len() {
        let (name, base) = modules[index].clone();
        if let Some(definition) = mine.iter().find(|d| last(&d.from) == name) {
            for include in &definition.includes {
                let path = rebase(include, &base);
                if !modules.iter().any(|(known, _)| *known == last(&path)) {
                    modules.push((last(&path), path));
                }
            }
        }
        index += 1;
    }
    modules
}

/// The next group this selection still needs members from, handed the whole
/// application; `None` once every group it names or includes has answered.
///
/// A group's macro is exported at its crate's root whatever module the group
/// is in, so it is called there, by the module's name.
pub(crate) fn next_callback(
    group: &GroupInstance,
    definitions: &[&GroupDefinition],
    uses: &[&ItemUse],
    original: &TokenStream,
) -> Option<TokenStream> {
    let mine = definitions_for(group, definitions);
    let name = &group.name;
    group_modules(group, &mine, uses)
        .into_iter()
        .find(|(module, _)| !mine.iter().any(|d| last(&d.from) == *module))
        .map(|(module, path)| {
            let leading = path.leading_colon;
            let krate = &path.segments[0].ident;
            let module = format_ident!("{}", module);
            quote!(#leading #krate::#module! { #name ; #original })
        })
}

fn unique<'a>(names: impl Iterator<Item = &'a Ident>) -> Vec<&'a Ident> {
    let mut found: Vec<&Ident> = Vec::new();
    for name in names {
        if !found.contains(&name) {
            found.push(name);
        }
    }
    found
}

fn list(names: &[&Ident]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Check that the firmware binds each of the group's names exactly once, and
/// nothing else.
fn check_bindings(
    kind: &str,
    group: &GroupInstance,
    needed: &[&Ident],
    given: &[&Ident],
) -> syn::Result<()> {
    for (index, name) in given.iter().enumerate() {
        if given[..index].contains(name) {
            return Err(syn::Error::new(
                name.span(),
                format!("{kind} `{name}` is bound twice"),
            ));
        }
        if !needed.contains(name) {
            return Err(syn::Error::new(
                name.span(),
                format!(
                    "`{name}` is not a {kind} name of this group; it has {}",
                    match needed.is_empty() {
                        true => "none".to_string(),
                        false => list(needed),
                    }
                ),
            ));
        }
    }
    let missing = needed
        .iter()
        .filter(|name| !given.contains(name))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(syn::Error::new(
            group.from.span(),
            format!(
                "this group needs {kind} {} bound as well: `{kind} = [{} = ..]`",
                list(&missing),
                missing[0]
            ),
        ));
    }
    Ok(())
}

/// One ordinary instance per member, named `<selection>_<member>`, with every
/// requirement bound through the group's namespace to the firmware's resource.
pub(crate) fn instances(
    group: &GroupInstance,
    definitions: &[&GroupDefinition],
    uses: &[&ItemUse],
) -> syn::Result<(Vec<Instance>, TokenStream)> {
    // A member reached through two includes is still one member. Its paths
    // are made absolute against the module it came from.
    let modules = group_modules(group, definitions, uses);
    let mut owned: Vec<Member> = Vec::new();
    for definition in definitions {
        let base = &modules
            .iter()
            .find(|(name, _)| *name == last(&definition.from))
            .expect("every definition was asked for by module")
            .1;
        for member in &definition.members {
            if owned.iter().any(|m| m.name() == member.name()) {
                continue;
            }
            let mut member = member.clone();
            member.from = member.from.map(|from| rebase(&from, base));
            for input in member.signature.inputs.iter_mut() {
                if let syn::FnArg::Typed(typed) = input
                    && let syn::Type::Path(ty) = typed.ty.as_mut()
                {
                    ty.path = rebase(&ty.path, base);
                }
            }
            owned.push(member);
        }
    }
    let members = &owned.iter().collect::<Vec<_>>();
    let names = members.iter().map(|m| m.name()).collect::<Vec<_>>();

    let needed_shared = unique(members.iter().flat_map(|m| &m.shared).map(|r| &r.target));
    let needed_local = unique(members.iter().flat_map(|m| &m.local).map(|r| &r.target));
    let needed_config = unique(members.iter().flat_map(|m| &m.config).map(|r| &r.target));
    let needed_spawn = unique(
        members
            .iter()
            .flat_map(|m| &m.spawn)
            .map(|r| &r.target)
            .filter(|target| !names.contains(target)),
    );
    let given_shared = group
        .shared
        .iter()
        .map(|r| &r.requirement)
        .collect::<Vec<_>>();
    let given_local = group.local.iter().map(|l| l.names().0).collect::<Vec<_>>();
    let given_config = group.config.iter().map(|c| &c.name).collect::<Vec<_>>();
    let given_spawn = group
        .spawn
        .iter()
        .map(|r| &r.requirement)
        .collect::<Vec<_>>();
    check_bindings("shared", group, &needed_shared, &given_shared)?;
    check_bindings("local", group, &needed_local, &given_local)?;
    check_bindings("config", group, &needed_config, &given_config)?;
    check_bindings("spawn", group, &needed_spawn, &given_spawn)?;

    for (index, settings) in group.tasks.iter().enumerate() {
        if !names.contains(&&settings.member) {
            return Err(syn::Error::new(
                settings.member.span(),
                format!(
                    "`{}` is not a task of this group; its tasks are {}",
                    settings.member,
                    list(&names)
                ),
            ));
        }
        if group.tasks[..index]
            .iter()
            .any(|other| other.member == settings.member)
        {
            return Err(syn::Error::new(
                settings.member.span(),
                format!("`{}` is given settings twice", settings.member),
            ));
        }
    }

    let prefix = &group.name;
    let rename = |member: &Ident| format_ident!("{}_{}", prefix, member, span = prefix.span());

    let mut instances = Vec::new();
    for member in members {
        let name = member.name();
        let settings = group
            .tasks
            .iter()
            .find(|settings| &settings.member == name)
            .ok_or_else(|| {
                syn::Error::new(
                    group.from.span(),
                    format!(
                        "this group's `{name}` is not declared: add `{}` to the module",
                        match member.signature.asyncness {
                            None =>
                                format!("#[task(binds = <interrupt>, priority = ..)] fn {name};"),
                            Some(_) => format!("#[task(priority = ..)] async fn {name};"),
                        }
                    ),
                )
            })?;
        match (&member.signature.asyncness, &settings.asyncness) {
            (Some(_), None) => {
                return Err(syn::Error::new(
                    settings.member.span(),
                    format!("`{name}` is a software task: declare it `async fn {name};`"),
                ));
            }
            (None, Some(asyncness)) => {
                return Err(syn::Error::new(
                    asyncness.span,
                    format!("`{name}` is a hardware task: declare it `fn {name};`"),
                ));
            }
            _ => {}
        }
        if member.signature.asyncness.is_none() && settings.binds.is_none() {
            return Err(syn::Error::new(
                settings.member.span(),
                format!("`{name}` is a hardware task; bind it: `#[task(binds = <interrupt>, ..)]`"),
            ));
        }

        let shared = member
            .shared
            .iter()
            .map(|r| {
                let bound = group.shared.iter().find(|b| b.requirement == r.target);
                Rebind {
                    requirement: r.requirement.clone(),
                    target: bound.expect("checked above").target.clone(),
                }
            })
            .collect();
        let local = member
            .local
            .iter()
            .map(|r| {
                let bound = group.local.iter().find(|b| b.names().0 == &r.target);
                match bound.expect("checked above") {
                    LocalBinding::Resource(rebind) => LocalBinding::Resource(Rebind {
                        requirement: r.requirement.clone(),
                        target: rebind.target.clone(),
                    }),
                    LocalBinding::TaskLocal {
                        name, ty, value, ..
                    } => LocalBinding::TaskLocal {
                        requirement: r.requirement.clone(),
                        name: name.clone(),
                        ty: ty.clone(),
                        value: value.clone(),
                    },
                }
            })
            .collect();
        let spawn = member
            .spawn
            .iter()
            .map(|r| Rebind {
                requirement: r.requirement.clone(),
                target: match names.contains(&&r.target) {
                    true => rename(&r.target),
                    false => group
                        .spawn
                        .iter()
                        .find(|b| b.requirement == r.target)
                        .expect("checked above")
                        .target
                        .clone(),
                },
            })
            .collect();
        let config = member
            .config
            .iter()
            .map(|r| {
                let value = group.config.iter().find(|c| c.name == r.target);
                ConfigValue {
                    name: r.requirement.clone(),
                    ..value.expect("checked above").clone()
                }
            })
            .collect();

        let instance_name = rename(name);
        let mut signature = member.signature.clone();
        signature.ident = instance_name.clone();
        let cx = format_ident!("cx");
        signature
            .inputs
            .insert(0, syn::parse_quote!(#cx: #instance_name::Context));

        instances.push(Instance {
            attribute: TaskAttribute {
                from: member.from.clone(),
                priority: settings.priority.clone(),
                binds: settings.binds.clone(),
                local,
                shared,
                config,
                spawn,
            },
            attributes: group
                .attributes
                .iter()
                .chain(&settings.attributes)
                .cloned()
                .collect(),
            signature,
        });
    }

    // The selection is a real module, so a member is named as it was declared:
    // `sbus_link::parse::spawn(..)`, as RTIC names any task.
    let cfgs = group
        .attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"));
    let aliases = members.iter().map(|member| {
        let (member, task) = (member.name(), rename(member.name()));
        quote!(pub use super::#task as #member;)
    });
    let module = quote! {
        #(#cfgs)*
        pub mod #prefix {
            #(#aliases)*
        }
    };
    Ok((instances, module))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, expand};

    fn defined(arguments: &str, module: &str) -> syn::Result<String> {
        define(syn::parse_str(arguments)?, syn::parse_str(module)?).map(|o| o.to_string())
    }

    const RX: &str = "pub mod rx {
        use super::*;
        #[ferroforge::task(shared = [port: Port], local = [uart: Uart], spawn = [frame(n: usize)])]
        pub fn on_uart(cx: on_uart::Context) {}
        #[ferroforge::task(spawn = [decoded(n: usize)])]
        pub async fn parse(cx: parse::Context, n: usize) {}
    }";

    /// What `both`'s macro appends, and what `rx`'s appends after it.
    const BOTH: &str = "@group link from both { use super::rx; use super::tx; }
        @group link from rx {
            #[task(from = self::on_uart, shared = [port = port], local = [uart = uart],
                   spawn = [frame = parse], config = [])] fn on_uart();
            #[task(from = self::parse, shared = [], local = [], spawn = [decoded = decoded],
                   config = [])] async fn parse(n: self::__ff_parse_input_0);
        }";
    const TX: &str = "@group link from tx {
        #[task(from = self::on_tx, shared = [], local = [stream = stream], spawn = [],
               config = [])] fn on_tx();
    }";

    const SELECTION: &str = "#[group(from = lib::both, shared = [port = uart],
            local = [uart = usart, stream = tx_stream], spawn = [decoded = sbus])]
        mod link {
            #[task(binds = USART1, priority = 3)] fn on_uart;
            #[task(binds = DMA1, priority = 2)] fn on_tx;
            #[task(priority = 1)] async fn parse;
        }";

    fn application(selection: &str, definitions: &str) -> syn::Result<String> {
        let source = format!("device = chip::pac, use my_lib as lib;\n{selection}\n{definitions}");
        expand(syn::parse_str::<App>(&source)?).map(|output| output.to_string())
    }

    #[test]
    fn a_group_reads_its_members_from_the_definitions() {
        let output = defined("spawn = [frame = parse]", RX).unwrap();
        assert!(output.contains("pub mod rx"), "the module stays: {output}");
        assert!(output.contains("macro_rules ! rx"), "{output}");
        assert!(output.contains("from = self :: on_uart"), "{output}");
        assert!(output.contains("frame = parse"), "wired inside: {output}");
        assert!(output.contains("decoded = decoded"), "left open: {output}");
        // The context is the adapter's; only the task's own inputs are carried,
        // each through an alias the firmware can name by path.
        assert!(
            output.contains("async fn parse (n : self :: __ff_parse_input_0) ;"),
            "{output}"
        );
        assert!(
            output.contains("pub type __ff_parse_input_0 = usize ;"),
            "{output}"
        );
        // `use super::*;` is an import, not an included group.
        assert!(!output.contains("use super ;"), "{output}");
    }

    #[test]
    fn a_definitions_own_locals_are_not_the_firmwares_to_bind() {
        let output = defined(
            "",
            "pub mod tx { #[ferroforge::task(local = [stream: S, sent: u32 = 0])] \
             pub fn on_tx(cx: on_tx::Context) {} }",
        )
        .unwrap();
        assert!(output.contains("local = [stream = stream]"), "{output}");
    }

    #[test]
    fn a_pub_use_glob_includes_another_group() {
        let output = defined(
            "",
            "pub mod both { pub use super::rx::*; pub use super::tx::*; }",
        )
        .unwrap();
        assert!(output.contains("use super :: rx ;"), "{output}");
        assert!(output.contains("use super :: tx ;"), "{output}");
    }

    #[test]
    fn two_members_cannot_need_one_local() {
        let error = defined(
            "",
            "pub mod g { #[ferroforge::task(local = [uart: U])] pub fn a(cx: a::Context) {} \
             #[ferroforge::task(local = [uart: U])] pub fn b(cx: b::Context) {} }",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("both need a local called `uart`"), "{error}");
    }

    #[test]
    fn a_selection_asks_each_group_it_names_or_includes_in_turn() {
        let output = application(SELECTION, "").unwrap();
        assert!(output.starts_with("my_lib :: both ! { link ;"), "{output}");
        // An included group is asked at its crate's root, by name.
        let output = application(SELECTION, BOTH).unwrap();
        assert!(output.starts_with("my_lib :: tx ! { link ;"), "{output}");
    }

    #[test]
    fn a_complete_selection_becomes_one_instance_per_member() {
        let output = application(SELECTION, &format!("{BOTH} {TX}")).unwrap();
        for task in ["fn link_on_uart", "async fn link_parse", "fn link_on_tx"] {
            assert!(output.contains(task), "{task}: {output}");
        }
        assert!(output.contains("frame : link_parse :: spawn"), "{output}");
        // Paths come back absolute, against where the firmware found the group.
        assert!(output.contains("my_lib :: rx :: on_uart"), "{output}");
        assert!(
            output.contains("n : my_lib :: rx :: __ff_parse_input_0"),
            "{output}"
        );
        assert!(output.contains("decoded : sbus :: spawn"), "{output}");
        assert!(output.contains("port : cx . shared . uart"), "{output}");
        assert!(
            output.contains("stream : cx . local . tx_stream"),
            "{output}"
        );
        assert!(output.contains("binds = USART1 , priority = 3"), "{output}");
        // Each member is reachable by the name it was declared with.
        assert!(
            output.contains("pub mod link { pub use super :: link_on_uart as on_uart ;"),
            "{output}"
        );
    }

    #[test]
    fn every_group_name_is_bound_once() {
        for (from, to, message) in [
            ("spawn = [decoded = sbus]", "", "needs spawn `decoded`"),
            (
                "shared = [port = uart]",
                "shared = [port = uart, extra]",
                "`extra` is not a shared name",
            ),
            (
                "shared = [port = uart]",
                "shared = [port = uart, port = b]",
                "bound twice",
            ),
        ] {
            let error = application(&SELECTION.replace(from, to), &format!("{BOTH} {TX}"))
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{to}: {error}");
        }
    }

    #[test]
    fn every_member_is_declared_as_the_task_it_is() {
        for (from, to, message) in [
            (
                "#[task(binds = DMA1, priority = 2)] fn on_tx;",
                "",
                "`on_tx` is not declared",
            ),
            (
                "binds = DMA1, priority = 2",
                "priority = 2",
                "is a hardware task; bind it",
            ),
            (
                "async fn parse;",
                "fn parse;",
                "declare it `async fn parse;`",
            ),
            ("fn on_tx;", "async fn on_tx;", "declare it `fn on_tx;`"),
            (
                "async fn parse;",
                "async fn parse; fn idle;",
                "not a task of this group",
            ),
        ] {
            let error = application(&SELECTION.replace(from, to), &format!("{BOTH} {TX}"))
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{to}: {error}");
        }
    }

    #[test]
    fn a_group_may_sit_anywhere_in_its_crate() {
        let base: Path = syn::parse_quote!(lib::serial::both);
        for (relative, absolute) in [
            ("self::on_uart", "lib :: serial :: both :: on_uart"),
            ("super::rx", "lib :: serial :: rx"),
            ("crate::rx", "lib :: rx"),
            ("other::rx", "other :: rx"),
        ] {
            let path: Path = syn::parse_str(relative).unwrap();
            assert_eq!(rebase(&path, &base).to_token_stream().to_string(), absolute);
        }
        let selection = SELECTION.replace("from = lib::both", "from = lib::serial::both");
        let output = application(&selection, BOTH).unwrap();
        assert!(output.starts_with("my_lib :: tx ! { link ;"), "{output}");
        let output = application(&selection, &format!("{BOTH} {TX}")).unwrap();
        assert!(
            output.contains("my_lib :: serial :: rx :: on_uart"),
            "{output}"
        );
    }
}
