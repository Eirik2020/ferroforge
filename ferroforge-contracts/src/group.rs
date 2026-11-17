//! What a task group declares, parsed and checked for the macros and for host
//! tools.
//!
//! `#[ferroforge::group(..)]` takes two kinds of argument. `spawn` wires one
//! member's outgoing call to another member, as it always has. The rest is the
//! group's contract: what a firmware, or a tool writing one, must supply to
//! select the group without knowing which group it is.
//!
//! ```text
//! #[ferroforge::group(
//!     slots = [instance: U, tx: Tx, rx: Rx, tx_dma: TxD, rx_dma: RxD],
//!     vectors = [on_uart = instance, on_rx_dma = rx_dma, on_tx_dma = tx_dma],
//!     shared = [uart: Uart<Wiring<U, Tx, Rx, TxD, RxD>>],
//!     constructor = new,
//! )]
//! pub mod uart_port { .. }
//! ```
//!
//! - `slots` names the hardware the group is built from, by role, and the
//!   constructor's type parameter that carries each one. A tool fills a slot
//!   with the type the board and chip say serves that role.
//! - `vectors` says which member serves the interrupt of which slot, so the
//!   member's `binds` follows from the slot's type.
//! - `shared` and `local` list every resource the members leave for the
//!   firmware to bind, with its type, written over the slots' parameters.
//! - `constructor` is the function that builds those resources. It returns
//!   them as one struct with a public field per resource, of the same name.
//!
//! Nothing here reads a function body, and nothing depends on a HAL: the
//! contract is read from source, by the macro and by any host tool, through
//! [`GroupArguments`] and [`groups`].

use std::collections::BTreeSet;

use syn::{
    Attribute, Error, Ident, Item, ItemMod, Path, Token, Visibility,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

use crate::{Resource, TaskArguments, identifier_key, list};

/// One outgoing call a group wires to one of its members, `frame = parse`.
#[derive(Clone, Debug)]
pub struct Wire {
    /// The member's outgoing call, as the member declares it.
    pub call: Ident,
    /// The member it reaches.
    pub task: Ident,
}

impl Parse for Wire {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let call = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            call,
            task: input.parse()?,
        })
    }
}

/// A hardware slot: a role the group needs filled, `tx_dma: TxD`, and the
/// constructor's type parameter that carries it.
#[derive(Clone, Debug)]
pub struct Slot {
    /// The role, in the group's words: `instance`, `tx`, `rx_dma`.
    pub role: Ident,
    /// The constructor's type parameter for it, which resource types use.
    pub parameter: Ident,
}

impl Parse for Slot {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let role = input.parse()?;
        input.parse::<Token![:]>()?;
        Ok(Self {
            role,
            parameter: input.parse()?,
        })
    }
}

/// Which member serves which slot's interrupt, `on_rx_dma = rx_dma`.
#[derive(Clone, Debug)]
pub struct VectorRole {
    /// The hardware member bound to the vector.
    pub member: Ident,
    /// The slot whose interrupt it is.
    pub slot: Ident,
}

impl Parse for VectorRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let member = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            member,
            slot: input.parse()?,
        })
    }
}

/// What a group needs from whoever selects it: its slots, the vectors its
/// members serve, the resources it leaves to the firmware, and the function
/// that builds them.
#[derive(Clone, Debug, Default)]
pub struct GroupContract {
    /// `slots = [..]`: the hardware it is built from, by role.
    pub slots: Vec<Slot>,
    /// `vectors = [..]`: which member serves which slot's interrupt.
    pub vectors: Vec<VectorRole>,
    /// `shared = [..]`: the shared resources the firmware binds, typed.
    pub shared: Vec<Resource>,
    /// `local = [..]`: the locals the firmware binds, typed.
    pub local: Vec<Resource>,
    /// `constructor = ..`: what builds the resources, relative to the group.
    pub constructor: Option<Path>,
}

impl GroupContract {
    /// Whether the group states a contract at all. A group without one is a
    /// plain set of tasks, as before contracts existed.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
            && self.vectors.is_empty()
            && self.shared.is_empty()
            && self.local.is_empty()
            && self.constructor.is_none()
    }

    /// Every resource the contract lists, shared ones first.
    pub fn resources(&self) -> impl Iterator<Item = &Resource> {
        self.shared.iter().chain(&self.local)
    }

    /// The structure of the contract on its own, before any member is read:
    /// duplicates, unknown slots, missing types, and a constructor.
    pub fn validate(&self, span: proc_macro2::Span) -> syn::Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        unique(self.slots.iter().map(|s| &s.role), "duplicate slot")?;
        unique(
            self.slots.iter().map(|s| &s.parameter),
            "two slots cannot share a type parameter",
        )?;
        unique(
            self.vectors.iter().map(|v| &v.member),
            "a member serves one vector; select it again for another",
        )?;
        unique(
            self.resources().map(|r| &r.name),
            "duplicate resource; a resource is shared or local, once",
        )?;
        for vector in &self.vectors {
            if !self
                .slots
                .iter()
                .any(|slot| identifier_key(&slot.role) == identifier_key(&vector.slot))
            {
                return Err(Error::new(
                    vector.slot.span(),
                    format!("`{}` is not one of this group's `slots`", vector.slot),
                ));
            }
        }
        for resource in self.resources() {
            if resource.ty.is_none() {
                return Err(Error::new(
                    resource.name.span(),
                    "a contract resource states its type, `name: Type`, because \
                     whoever selects the group declares the field",
                ));
            }
            if let Some(init) = &resource.init {
                return Err(Error::new(
                    init.span(),
                    "a contract resource has no initial value: the constructor \
                     builds it; a member's own state stays on the member",
                ));
            }
            if let Some(cfg) = resource.cfgs.first() {
                return Err(Error::new(
                    cfg.span(),
                    "a contract resource cannot be `#[cfg]`-gated",
                ));
            }
        }
        for resource in &self.local {
            if let Some(span) = resource.lock_free {
                return Err(Error::new(
                    span,
                    "only a shared resource can be `#[lock_free]`",
                ));
            }
        }
        if self.constructor.is_none() {
            return Err(Error::new(
                span,
                "a group with a contract names its `constructor`, the function \
                 that builds its resources",
            ));
        }
        Ok(())
    }
}

/// Everything a `#[ferroforge::group(..)]` attribute declares.
#[derive(Clone, Debug, Default)]
pub struct GroupArguments {
    /// `spawn = [..]`: calls wired between members.
    pub spawn: Vec<Wire>,
    /// The rest: the group's contract, empty when it states none.
    pub contract: GroupContract,
}

impl Parse for GroupArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut arguments = Self::default();
        let mut keys = BTreeSet::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if !keys.insert(key.to_string()) {
                return Err(Error::new(
                    key.span(),
                    format!("`{key}` appears more than once"),
                ));
            }
            input.parse::<Token![=]>()?;
            let contract = &mut arguments.contract;
            match key.to_string().as_str() {
                "spawn" => arguments.spawn = list(input)?,
                "slots" => contract.slots = list(input)?,
                "vectors" => contract.vectors = list(input)?,
                "shared" => contract.shared = list(input)?,
                "local" => contract.local = list(input)?,
                "constructor" => contract.constructor = Some(input.parse()?),
                other => {
                    return Err(Error::new(
                        key.span(),
                        format!(
                            "unknown group argument `{other}`; a group wires `spawn` \
                             between its members and may state its contract - \
                             `slots`, `vectors`, `shared`, `local` and `constructor` - \
                             and the firmware sets the rest"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(arguments)
    }
}

fn unique<'a>(names: impl IntoIterator<Item = &'a Ident>, message: &str) -> syn::Result<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(identifier_key(name)) {
            return Err(Error::new(name.span(), message));
        }
    }
    Ok(())
}

/// One task definition directly inside a group module.
#[derive(Clone, Debug)]
pub struct GroupMember {
    /// The function's name.
    pub name: Ident,
    /// What its `#[task(..)]` declares.
    pub arguments: TaskArguments,
    /// `async fn` or `fn`.
    pub kind: crate::TaskKind,
}

/// A group as a host tool reads it from source: where it is, its arguments,
/// and the members defined directly inside it. Members it includes by `pub use`
/// are in the modules those name, which a tool reads from their own source.
#[derive(Clone, Debug)]
pub struct GroupDeclaration {
    /// The module path from the file's root, the group's own name last.
    pub path: Vec<Ident>,
    /// Its `#[ferroforge::group(..)]` arguments.
    pub arguments: GroupArguments,
    /// The task definitions directly inside it.
    pub members: Vec<GroupMember>,
}

/// Every group declared in one source file, inline modules at any depth
/// included, for a host tool reading descriptors from source. A group's own
/// contract is checked as the macro checks it; members are read, not expanded.
pub fn groups(file: &syn::File) -> syn::Result<Vec<GroupDeclaration>> {
    let mut found = Vec::new();
    walk(&file.items, &mut Vec::new(), &mut found)?;
    Ok(found)
}

fn walk(
    items: &[Item],
    path: &mut Vec<Ident>,
    found: &mut Vec<GroupDeclaration>,
) -> syn::Result<()> {
    for item in items {
        let Item::Mod(module) = item else { continue };
        let Some((_, content)) = &module.content else {
            continue;
        };
        path.push(module.ident.clone());
        if let Some(attribute) = module.attrs.iter().find(|a| is_named(a, "group")) {
            found.push(declaration(module, attribute, path.clone())?);
        }
        walk(content, path, found)?;
        path.pop();
    }
    Ok(())
}

fn is_named(attribute: &Attribute, name: &str) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

fn declaration(
    module: &ItemMod,
    attribute: &Attribute,
    path: Vec<Ident>,
) -> syn::Result<GroupDeclaration> {
    let arguments: GroupArguments = match &attribute.meta {
        syn::Meta::Path(_) => GroupArguments::default(),
        _ => attribute.parse_args()?,
    };
    arguments.contract.validate(module.ident.span())?;
    let mut members = Vec::new();
    for item in module.content.iter().flat_map(|(_, items)| items) {
        let Item::Fn(function) = item else { continue };
        if !matches!(function.vis, Visibility::Public(_)) {
            continue;
        }
        let Some(task) = function.attrs.iter().find(|a| is_named(a, "task")) else {
            continue;
        };
        let task_arguments: TaskArguments = match &task.meta {
            syn::Meta::Path(_) => TaskArguments::default(),
            _ => task.parse_args()?,
        };
        members.push(GroupMember {
            name: function.sig.ident.clone(),
            arguments: task_arguments,
            kind: if function.sig.asyncness.is_some() {
                crate::TaskKind::Software
            } else {
                crate::TaskKind::Hardware
            },
        });
    }
    Ok(GroupDeclaration {
        path,
        arguments,
        members,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UART_PORT: &str = r#"
        pub mod ports {
            /// A UART port.
            #[ferroforge::group(
                slots = [instance: U, tx: Tx, rx: Rx, tx_dma: TxD, rx_dma: RxD],
                vectors = [on_uart = instance, on_rx_dma = rx_dma, on_tx_dma = tx_dma],
                shared = [uart: Uart<Wiring<U, Tx, Rx, TxD, RxD>>],
                constructor = new,
            )]
            pub mod uart_port {
                #[ferroforge::task(shared = [uart], bounds = [uart: Service])]
                pub fn on_uart(cx: on_uart::Context) {}
                #[ferroforge::task(shared = [uart], bounds = [uart: Service])]
                pub fn on_rx_dma(cx: on_rx_dma::Context) {}
                #[ferroforge::task(shared = [uart], bounds = [uart: Service])]
                pub fn on_tx_dma(cx: on_tx_dma::Context) {}
                fn helper() {}
            }
        }
    "#;

    fn contract(arguments: &str) -> syn::Result<GroupArguments> {
        let arguments: GroupArguments = syn::parse_str(arguments)?;
        arguments
            .contract
            .validate(proc_macro2::Span::call_site())?;
        Ok(arguments)
    }

    fn refused(arguments: &str) -> String {
        contract(arguments)
            .expect_err("the contract should be refused")
            .to_string()
    }

    #[test]
    fn a_host_tool_reads_a_groups_contract_and_members_from_source() {
        let file: syn::File = syn::parse_str(UART_PORT).unwrap();
        let found = groups(&file).unwrap();
        assert_eq!(found.len(), 1);
        let group = &found[0];
        let path: Vec<String> = group.path.iter().map(ToString::to_string).collect();
        assert_eq!(path, ["ports", "uart_port"]);
        let contract = &group.arguments.contract;
        let roles: Vec<String> = contract.slots.iter().map(|s| s.role.to_string()).collect();
        assert_eq!(roles, ["instance", "tx", "rx", "tx_dma", "rx_dma"]);
        assert_eq!(contract.vectors[1].member, "on_rx_dma");
        assert_eq!(contract.vectors[1].slot, "rx_dma");
        assert_eq!(contract.shared[0].name, "uart");
        assert!(contract.constructor.as_ref().unwrap().is_ident("new"));
        let members: Vec<String> = group.members.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(members, ["on_uart", "on_rx_dma", "on_tx_dma"]);
        assert!(
            group
                .members
                .iter()
                .all(|m| m.kind == crate::TaskKind::Hardware)
        );
    }

    #[test]
    fn a_group_without_a_contract_still_reads() {
        let arguments = contract("spawn = [frame = parse]").unwrap();
        assert!(arguments.contract.is_empty());
        assert_eq!(arguments.spawn[0].call, "frame");
        assert_eq!(arguments.spawn[0].task, "parse");
    }

    #[test]
    fn a_contract_names_its_constructor() {
        assert!(refused("shared = [uart: Uart]").contains("constructor"));
    }

    #[test]
    fn a_vector_names_a_slot() {
        assert!(
            refused("slots = [instance: U], vectors = [on_uart = irq], constructor = new")
                .contains("`irq` is not one of this group's `slots`")
        );
    }

    #[test]
    fn a_contract_resource_states_its_type() {
        assert!(refused("shared = [uart], constructor = new").contains("states its type"));
    }

    #[test]
    fn a_contract_resource_has_no_initial_value() {
        assert!(refused("local = [n: u32 = 0], constructor = new").contains("no initial value"));
    }

    #[test]
    fn a_resource_is_listed_once() {
        assert!(
            refused("shared = [a: u8], local = [a: u8], constructor = new")
                .contains("duplicate resource")
        );
    }

    #[test]
    fn slots_are_distinct() {
        assert!(refused("slots = [a: U, b: U], constructor = new").contains("type parameter"));
        assert!(refused("slots = [a: U, a: V], constructor = new").contains("duplicate slot"));
    }

    #[test]
    fn a_local_is_never_lock_free() {
        assert!(refused("local = [#[lock_free] a: u8], constructor = new").contains("lock_free"));
    }

    #[test]
    fn an_unknown_argument_names_the_ones_there_are() {
        let error = syn::parse_str::<GroupArguments>("binds = [x]")
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("`slots`, `vectors`, `shared`, `local` and `constructor`"));
    }
}
