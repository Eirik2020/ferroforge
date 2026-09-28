//! Task groups: tasks that only work as a set, selected by one declaration.
//!
//! A library states the set once with `ferroforge::group!`: each member's
//! definition, its signature past the context, and which of the group's names
//! each of its requirements binds to. A firmware then selects the whole set with
//! one `#[group(from = ..)] mod name;` inside `app!`, binding the union of the
//! group's resources once and giving each member its priority and interrupt.
//!
//! `app!` still reads nothing but its own input. `#[rtic::app]` must see every
//! task, so the members have to reach `app!` as tokens, and they come through a
//! callback: `group!` turns each group into a `macro_rules!` holding its
//! members, and `app!`, finding a group it has no members for, hands the whole
//! application to that macro, which appends the members and calls `app!` again.
//!
//! ```text
//! app! { .. #[group(from = lib::g)] mod x; .. }
//!   -> lib::g! { x ; .. #[group(from = lib::g)] mod x; .. }
//!   -> app! { .. #[group(from = lib::g)] mod x; .. @group x { members } }
//!   -> #[rtic::app] mod app { .. x_member_a .. x_member_b .. }
//! ```

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{
    Attribute, Ident, ItemUse, LitInt, Path, Signature, Token, UseTree,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
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
    /// the definition's path made absolute through `$crate`.
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
                from = $crate::#from,
                shared = [#(#shared),*],
                local = [#(#local),*],
                spawn = [#(#spawn),*],
                config = [#(#config),*],
            )]
            #signature;
        }
    }
}

/// `pub group name = [member, other_group, ..];`
struct GroupDeclaration {
    docs: Vec<Attribute>,
    name: Ident,
    entries: Vec<Ident>,
}

/// Everything one `group!` holds: members stated once, then any number of
/// named sets of them. A set may name an earlier set, so `uart_dma` can be
/// `[uart_dma_rx, uart_dma_tx]` without restating either.
pub struct Library {
    members: Vec<Member>,
    groups: Vec<GroupDeclaration>,
}

impl Parse for Library {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut members = Vec::new();
        let mut groups = Vec::new();
        while !input.is_empty() {
            let fork = input.fork();
            fork.call(Attribute::parse_outer)?;
            fork.parse::<syn::Visibility>()?;
            let is_group = fork.peek(Ident) && fork.parse::<Ident>()? == "group";
            if !is_group {
                members.push(input.parse()?);
                continue;
            }
            let docs = input.call(Attribute::parse_outer)?;
            input.parse::<syn::Visibility>()?;
            input.parse::<Ident>()?;
            let name = input.parse()?;
            input.parse::<Token![=]>()?;
            let content;
            syn::bracketed!(content in input);
            let entries = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            input.parse::<Token![;]>()?;
            groups.push(GroupDeclaration {
                docs,
                name,
                entries,
            });
        }
        Ok(Self { members, groups })
    }
}

/// `group!`: one exported `macro_rules!` per group, holding its members.
pub fn define(library: Library) -> syn::Result<TokenStream> {
    let Library { members, groups } = library;
    let mut resolved: Vec<(&Ident, Vec<&Member>)> = Vec::new();
    let mut output = TokenStream::new();

    for group in &groups {
        let mut chosen: Vec<&Member> = Vec::new();
        for entry in &group.entries {
            let found = if let Some(member) = members.iter().find(|m| m.name() == entry) {
                vec![member]
            } else if let Some((_, set)) = resolved.iter().find(|(name, _)| *name == entry) {
                set.clone()
            } else {
                return Err(syn::Error::new(
                    entry.span(),
                    format!("`{entry}` is neither a member nor an earlier group here"),
                ));
            };
            for member in found {
                if chosen.iter().any(|m| m.name() == member.name()) {
                    return Err(syn::Error::new(
                        entry.span(),
                        format!("`{}` is in this group twice", member.name()),
                    ));
                }
                chosen.push(member);
            }
        }

        // An RTIC local belongs to one task. Two members binding one group
        // local would compile here and fail inside RTIC; say so where the
        // group is written instead.
        for (index, member) in chosen.iter().enumerate() {
            for local in &member.local {
                if let Some(other) = chosen[index + 1..]
                    .iter()
                    .find(|other| other.local.iter().any(|l| l.target == local.target))
                {
                    return Err(syn::Error::new(
                        group.name.span(),
                        format!(
                            "`{}` and `{}` both bind local `{}`; a local belongs to one \
                             task, so share it instead",
                            member.name(),
                            other.name(),
                            local.target
                        ),
                    ));
                }
            }
        }

        let (docs, name) = (&group.docs, &group.name);
        let emitted = chosen.iter().map(|member| member.emit());
        output.extend(quote! {
            #(#docs)*
            #[macro_export]
            macro_rules! #name {
                ($instance:ident ; $($application:tt)*) => {
                    ::ferroforge::app! {
                        $($application)*
                        @group $instance { #(#emitted)* }
                    }
                };
            }
        });
        resolved.push((name, chosen));
    }
    Ok(output)
}

/// What a group's macro appended: `@group <selection> { members }`.
pub(crate) struct GroupDefinition {
    pub(crate) instance: Ident,
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
        let content;
        syn::braced!(content in input);
        let mut members = Vec::new();
        while !content.is_empty() {
            members.push(content.parse()?);
        }
        Ok(Self { instance, members })
    }
}

/// One member's firmware-side settings: `on_uart(binds = USART1, priority = 12)`.
struct TaskSettings {
    member: Ident,
    priority: Option<LitInt>,
    binds: Option<Path>,
}

impl Parse for TaskSettings {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let member = input.parse()?;
        let mut settings = Self {
            member,
            priority: None,
            binds: None,
        };
        if !input.peek(syn::token::Paren) {
            return Ok(settings);
        }
        let content;
        syn::parenthesized!(content in input);
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "priority" => settings.priority = Some(content.parse()?),
                "binds" => settings.binds = Some(content.parse()?),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown option `{other}`; a member takes `priority` and `binds`"),
                    ));
                }
            }
            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
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
///     local = [uart = usart, tx_stream, tx_sent: u32 = 0],
///     spawn = [decoded = sbus],
///     tasks = [on_uart(binds = USART1, priority = 12), parse(priority = 1)],
/// )]
/// mod sbus_link;
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
        let mut tasks = Vec::new();
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
                    "tasks" => tasks = bracketed_list(input)?,
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
        input.parse::<Token![;]>()?;
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

/// Hand the whole application to the group's own macro.
pub(crate) fn callback(
    group: &GroupInstance,
    uses: &[&ItemUse],
    original: &TokenStream,
) -> TokenStream {
    let from = resolve_alias(&group.from, uses);
    let name = &group.name;
    quote!(#from! { #name ; #original })
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
    definition: &GroupDefinition,
) -> syn::Result<Vec<Instance>> {
    let members = &definition.members;
    let names = members.iter().map(Member::name).collect::<Vec<_>>();

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
                        "this group's `{name}` has no settings: add `{name}({}priority = ..)` \
                         to `tasks = [..]`",
                        match member.signature.asyncness {
                            None => "binds = <interrupt>, ",
                            Some(_) => "",
                        }
                    ),
                )
            })?;
        if member.signature.asyncness.is_none() && settings.binds.is_none() {
            return Err(syn::Error::new(
                settings.member.span(),
                format!("`{name}` is a hardware task; bind it: `{name}(binds = <interrupt>, ..)`"),
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
            attributes: group.attributes.clone(),
            signature,
        });
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, expand};

    const LIBRARY: &str = "
        #[task(shared = [port], local = [uart], spawn = [frame = parse])] fn on_uart();
        #[task(local = [stream = tx_stream])] fn on_tx();
        #[task(spawn = [decoded])] async fn parse(bytes: usize);
        pub group rx = [on_uart, parse];
        pub group both = [rx, on_tx];
    ";

    fn defined(source: &str) -> syn::Result<String> {
        define(syn::parse_str(source)?).map(|output| output.to_string())
    }

    /// The members as `both`'s macro would append them.
    const BOTH: &str = "@group link {
        #[task(from = lib::on_uart, shared = [port = port], local = [uart = uart],
               spawn = [frame = parse])] fn on_uart();
        #[task(from = lib::parse, spawn = [decoded = decoded])] async fn parse(bytes: usize);
        #[task(from = lib::on_tx, local = [stream = tx_stream])] fn on_tx();
    }";

    fn application(selection: &str, definition: &str) -> syn::Result<String> {
        let source = format!("device = chip::pac, use my_lib as lib;\n{selection}\n{definition}");
        expand(syn::parse_str::<App>(&source)?).map(|output| output.to_string())
    }

    const SELECTION: &str = "#[group(from = lib::both, shared = [port = uart],
        local = [uart = usart, tx_stream: u32 = 0], spawn = [decoded = sbus],
        tasks = [on_uart(binds = USART1, priority = 3), on_tx(binds = DMA1, priority = 2),
                 parse(priority = 1)])] mod link;";

    #[test]
    fn each_group_is_an_exported_macro_holding_its_members() {
        let output = defined(LIBRARY).unwrap();
        assert!(output.contains("macro_rules ! rx"), "{output}");
        assert!(output.contains("macro_rules ! both"), "{output}");
        assert_eq!(output.matches("# [macro_export]").count(), 2, "{output}");
        // `both` names `rx`, so it holds rx's members and its own.
        let both = &output[output.find("macro_rules ! both").unwrap()..];
        for member in ["fn on_uart", "fn parse", "fn on_tx"] {
            assert!(both.contains(member), "{member}: {both}");
        }
        assert!(both.contains("from = $ crate :: on_tx"), "{both}");
    }

    #[test]
    fn two_members_cannot_share_a_local() {
        let error = defined(
            "#[task(local = [uart])] fn a(); #[task(local = [uart])] fn b(); group g = [a, b];",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("both bind local `uart`"), "{error}");
    }

    #[test]
    fn an_undefined_selection_hands_the_application_to_its_group() {
        let output = application(SELECTION, "").unwrap();
        assert!(output.starts_with("my_lib :: both ! { link ;"), "{output}");
        assert!(output.contains("mod link ;"), "{output}");
    }

    #[test]
    fn a_defined_selection_becomes_one_instance_per_member() {
        let output = application(SELECTION, BOTH).unwrap();
        for task in ["fn link_on_uart", "async fn link_parse", "fn link_on_tx"] {
            assert!(output.contains(task), "{task}: {output}");
        }
        // Wired inside the group, and to the firmware's own task outside it.
        assert!(output.contains("frame : link_parse :: spawn"), "{output}");
        assert!(output.contains("decoded : sbus :: spawn"), "{output}");
        // The group's names reach the firmware's resources.
        assert!(output.contains("port : cx . shared . uart"), "{output}");
        assert!(output.contains("uart : cx . local . usart"), "{output}");
        assert!(output.contains("tx_stream : u32 = 0"), "{output}");
        assert!(
            output.contains("stream : cx . local . tx_stream"),
            "{output}"
        );
        assert!(output.contains("binds = USART1 , priority = 3"), "{output}");
    }

    #[test]
    fn every_group_name_is_bound_once() {
        for (from, to, message) in [
            ("spawn = [decoded = sbus],", "", "needs spawn `decoded`"),
            (
                "shared = [port = uart],",
                "shared = [port = uart, extra],",
                "`extra` is not a shared name",
            ),
            (
                "shared = [port = uart],",
                "shared = [port = uart, port = b],",
                "bound twice",
            ),
        ] {
            let error = application(&SELECTION.replace(from, to), BOTH)
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{to}: {error}");
        }
    }

    #[test]
    fn every_member_is_given_its_settings() {
        for (from, to, message) in [
            (
                "on_tx(binds = DMA1, priority = 2),",
                "",
                "`on_tx` has no settings",
            ),
            (
                "on_tx(binds = DMA1, priority = 2)",
                "on_tx(priority = 2)",
                "is a hardware task",
            ),
            (
                "parse(priority = 1)",
                "parse(priority = 1), idle",
                "not a task of this group",
            ),
        ] {
            let error = application(&SELECTION.replace(from, to), BOTH)
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{to}: {error}");
        }
    }
}
