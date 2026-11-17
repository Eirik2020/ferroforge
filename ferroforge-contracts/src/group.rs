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
//!     buffers = [#[dma] tx_buf: [u8], #[dma] rx_buf: [u8]],
//!     config = [config: Config @ platform],
//!     constructor = new(rcc, wiring, config, tx_buf, rx_buf),
//! )]
//! pub mod uart_port { .. }
//! ```
//!
//! - `slots` names the hardware the group is built from, by role, and the
//!   constructor's type parameter that carries each one. A tool fills a slot
//!   with the type the board and chip say serves that role.
//! - `vectors` says which member serves the interrupt of which slot, so the
//!   member's `binds` follows from the slot's type. `member = [a, b]` says
//!   the member serves every vector the listed slots carry, one each: a slot
//!   such as a set of DMA lanes carries one vector per lane, and the
//!   firmware selects the member once per vector.
//! - `shared` and `local` list every resource the members leave for the
//!   firmware to bind, with its type, written over the slots' parameters.
//! - `buffers` lists the memory the constructor is handed, each a slice of an
//!   integer type. The selection sets each one's length; a tool emits it as a
//!   zeroed array in `init(local = [..])`. `#[dma]` marks a buffer that must
//!   sit in memory the DMA can reach.
//! - `config` lists the constructor's other inputs, each typed and tagged with
//!   the input layer that supplies its value: `board`, `profile`, `software`
//!   or `platform`.
//! - `constructor` is the function that builds those resources, with its
//!   arguments in order. Each argument is `rcc`, a slot passed on its own (a
//!   part the HAL's wiring does not hold, such as a chip-select pin),
//!   `wiring` (every other slot, as the HAL's wiring), a config field or a
//!   buffer. It returns the resources as one
//!   struct with a public field per resource, of the same name.
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

/// Which member serves which slot's interrupt, `on_rx_dma = rx_dma`, or
/// every vector of a set of slots, `on_lane_dma = [first_lanes,
/// second_lanes]`.
#[derive(Clone, Debug)]
pub struct VectorRole {
    /// The hardware member bound to the vector.
    pub member: Ident,
    /// The slot whose interrupt it is; in the bracketed form, the first slot
    /// listed.
    pub slot: Ident,
    /// The bracketed form's slots, `slot` first, each carrying any number of
    /// vectors: the member serves every one of them, and the firmware
    /// selects it once per vector, each with its own `binds`. Empty for the
    /// plain form, whose member serves exactly one vector.
    pub each: Vec<Ident>,
}

impl VectorRole {
    /// Every slot this vector names: `slot` alone, or the bracketed list.
    pub fn slots(&self) -> &[Ident] {
        match self.each.is_empty() {
            true => core::slice::from_ref(&self.slot),
            false => &self.each,
        }
    }

    /// Whether the member serves every vector of its slots, `member = [..]`.
    pub fn is_repeated(&self) -> bool {
        !self.each.is_empty()
    }
}

impl Parse for VectorRole {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let member = input.parse()?;
        input.parse::<Token![=]>()?;
        if input.peek(syn::token::Bracket) {
            let content;
            let brackets = syn::bracketed!(content in input);
            let each: Vec<Ident> = content
                .parse_terminated(Ident::parse, Token![,])?
                .into_iter()
                .collect();
            let Some(slot) = each.first().cloned() else {
                return Err(Error::new(
                    brackets.span.join(),
                    "list the slots whose vectors the member serves, `[lanes]`",
                ));
            };
            return Ok(Self { member, slot, each });
        }
        Ok(Self {
            member,
            slot: input.parse()?,
            each: Vec::new(),
        })
    }
}

/// Memory a group's constructor is handed, `#[dma] rx: [u8]`: a slice of an
/// integer type, whose length the selection sets. A tool emits it as a zeroed
/// array in `init(local = [..])` and passes it by name to the constructor.
#[derive(Clone, Debug)]
pub struct Buffer {
    /// Its name, as the constructor's argument list uses it.
    pub name: Ident,
    /// The element type, an integer primitive: `u8`, `u16`.
    pub element: Ident,
    /// `#[dma]`: it must sit in memory the DMA can reach.
    pub dma: bool,
    /// Its documentation.
    pub docs: Vec<Attribute>,
}

/// The element types a buffer may have: those a tool can zero with `0`.
const BUFFER_ELEMENTS: [&str; 8] = ["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64"];

impl Parse for Buffer {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut dma = false;
        let mut docs = Vec::new();
        for attribute in input.call(Attribute::parse_outer)? {
            match &attribute.meta {
                syn::Meta::Path(path) if path.is_ident("dma") => dma = true,
                syn::Meta::NameValue(meta) if meta.path.is_ident("doc") => docs.push(attribute),
                _ => {
                    return Err(Error::new(
                        attribute.span(),
                        "a buffer takes `#[dma]` and documentation, nothing else",
                    ));
                }
            }
        }
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: syn::Type = input.parse()?;
        let element = match &ty {
            syn::Type::Slice(slice) => match &*slice.elem {
                syn::Type::Path(path) if path.qself.is_none() => path.path.get_ident().cloned(),
                _ => None,
            },
            _ => None,
        }
        .filter(|element| BUFFER_ELEMENTS.contains(&element.to_string().as_str()))
        .ok_or_else(|| {
            Error::new(
                ty.span(),
                "a buffer is a slice of an integer type, `[u8]`: the selection \
                 sets its length, and a tool fills it with zeros",
            )
        })?;
        Ok(Self {
            name,
            element,
            dma,
            docs,
        })
    }
}

/// The input layer that supplies a config field's value (FerroSmith design
/// record v3, section 2). Live configuration is set at run time, after every
/// constructor has run, so it supplies none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// The board file: what the board's hardware fixes.
    Board,
    /// The MCU profile: what the chip and its clock tree fix.
    Profile,
    /// The software configuration, chosen when the firmware is built.
    Software,
    /// The platform configuration, read at boot.
    Platform,
}

impl Parse for Layer {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let layer: Ident = input.parse()?;
        match layer.to_string().as_str() {
            "board" => Ok(Self::Board),
            "profile" => Ok(Self::Profile),
            "software" => Ok(Self::Software),
            "platform" => Ok(Self::Platform),
            other => Err(Error::new(
                layer.span(),
                format!(
                    "`{other}` is not an input layer; a constructor's config comes from \
                     `board`, `profile`, `software` or `platform`"
                ),
            )),
        }
    }
}

/// A constructor input besides the wiring and buffers, `config: Config @ board`.
#[derive(Clone, Debug)]
pub struct ConfigField {
    /// Its name, as the constructor's argument list uses it.
    pub name: Ident,
    /// Its type, written as the constructor takes it.
    pub ty: syn::Type,
    /// The input layer that supplies its value.
    pub layer: Layer,
}

impl Parse for ConfigField {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![@]>()?;
        Ok(Self {
            name,
            ty,
            layer: input.parse()?,
        })
    }
}

/// The function that builds a group's resources, and its arguments in order,
/// `new(rcc, wiring, config, tx_buf, rx_buf)`.
#[derive(Clone, Debug)]
pub struct Constructor {
    /// Its path, relative to the group's module.
    pub path: Path,
    /// Its arguments, each `rcc`, `wiring`, a slot, a config field or a
    /// buffer.
    pub arguments: Vec<Ident>,
}

/// The constructor arguments that are neither a config field nor a buffer.
const FIXED_ARGUMENTS: [&str; 2] = ["rcc", "wiring"];

impl Parse for Constructor {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path: Path = input.parse()?;
        if !input.peek(syn::token::Paren) {
            return Err(Error::new(
                path.span(),
                "a constructor lists its arguments in order, `new(rcc, wiring, ..)`, \
                 so a tool can call it without knowing which group it is",
            ));
        }
        let body;
        syn::parenthesized!(body in input);
        let arguments = syn::punctuated::Punctuated::<Ident, Token![,]>::parse_terminated(&body)?
            .into_iter()
            .collect();
        Ok(Self { path, arguments })
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
    /// `buffers = [..]`: the memory the constructor is handed.
    pub buffers: Vec<Buffer>,
    /// `config = [..]`: the constructor's other inputs, with their layers.
    pub config: Vec<ConfigField>,
    /// `constructor = ..(..)`: what builds the resources, relative to the
    /// group, and its arguments in order.
    pub constructor: Option<Constructor>,
}

impl GroupContract {
    /// Whether the group states a contract at all. A group without one is a
    /// plain set of tasks, as before contracts existed.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
            && self.vectors.is_empty()
            && self.shared.is_empty()
            && self.local.is_empty()
            && self.buffers.is_empty()
            && self.config.is_empty()
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
            unique(vector.slots().iter(), "a slot is listed once")?;
            for named in vector.slots() {
                if !self
                    .slots
                    .iter()
                    .any(|slot| identifier_key(&slot.role) == identifier_key(named))
                {
                    return Err(Error::new(
                        named.span(),
                        format!("`{named}` is not one of this group's `slots`"),
                    ));
                }
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
        let Some(constructor) = &self.constructor else {
            return Err(Error::new(
                span,
                "a group with a contract names its `constructor`, the function \
                 that builds its resources",
            ));
        };
        self.validate_inputs(constructor)
    }

    /// The buffers and config fields against the constructor's arguments:
    /// each input named once, and each argument a known input.
    fn validate_inputs(&self, constructor: &Constructor) -> syn::Result<()> {
        let inputs: Vec<&Ident> = self
            .buffers
            .iter()
            .map(|b| &b.name)
            .chain(self.config.iter().map(|c| &c.name))
            .collect();
        unique(
            inputs
                .iter()
                .copied()
                .chain(self.slots.iter().map(|slot| &slot.role)),
            "duplicate input; a name is a slot, a buffer or a config field, once",
        )?;
        for input in inputs
            .iter()
            .copied()
            .chain(self.slots.iter().map(|slot| &slot.role))
        {
            if FIXED_ARGUMENTS.contains(&identifier_key(input).as_str()) {
                return Err(Error::new(
                    input.span(),
                    format!(
                        "`{input}` is the constructor's own argument; name the input otherwise"
                    ),
                ));
            }
        }
        unique(
            constructor.arguments.iter(),
            "the constructor takes each argument once",
        )?;
        for argument in &constructor.arguments {
            let key = identifier_key(argument);
            if !FIXED_ARGUMENTS.contains(&key.as_str())
                && !inputs.iter().any(|input| identifier_key(input) == key)
                && !self
                    .slots
                    .iter()
                    .any(|slot| identifier_key(&slot.role) == key)
            {
                return Err(Error::new(
                    argument.span(),
                    format!(
                        "`{argument}` is not `rcc`, `wiring`, one of this group's \
                         `slots`, nor one of its `buffers` or `config`"
                    ),
                ));
            }
        }
        for input in &inputs {
            if !constructor
                .arguments
                .iter()
                .any(|argument| identifier_key(argument) == identifier_key(input))
            {
                return Err(Error::new(
                    input.span(),
                    format!("the constructor's arguments leave out `{input}`"),
                ));
            }
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
                "buffers" => contract.buffers = list(input)?,
                "config" => contract.config = list(input)?,
                "constructor" => contract.constructor = Some(input.parse()?),
                other => {
                    return Err(Error::new(
                        key.span(),
                        format!(
                            "unknown group argument `{other}`; a group wires `spawn` \
                             between its members and may state its contract - \
                             `slots`, `vectors`, `shared`, `local`, `buffers`, `config` \
                             and `constructor` - and the firmware sets the rest"
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
                buffers = [#[dma] tx_buf: [u8], #[dma] rx_buf: [u8]],
                config = [config: Config @ platform],
                constructor = new(rcc, wiring, config, tx_buf, rx_buf),
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
        let constructor = contract.constructor.as_ref().unwrap();
        assert!(constructor.path.is_ident("new"));
        let arguments: Vec<String> = constructor
            .arguments
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(arguments, ["rcc", "wiring", "config", "tx_buf", "rx_buf"]);
        let buffers: Vec<(String, String, bool)> = contract
            .buffers
            .iter()
            .map(|b| (b.name.to_string(), b.element.to_string(), b.dma))
            .collect();
        assert_eq!(
            buffers,
            [
                ("tx_buf".to_string(), "u8".to_string(), true),
                ("rx_buf".to_string(), "u8".to_string(), true),
            ]
        );
        assert_eq!(contract.config[0].name, "config");
        assert_eq!(contract.config[0].layer, Layer::Platform);
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
            refused("slots = [instance: U], vectors = [on_uart = irq], constructor = new()")
                .contains("`irq` is not one of this group's `slots`")
        );
    }

    #[test]
    fn a_member_may_serve_every_vector_of_its_slots() {
        let arguments = contract(
            "slots = [first: A, second: B, first_lanes: LA, second_lanes: LB], \
             vectors = [on_lane_dma = [first_lanes, second_lanes]], \
             constructor = new(first, second, first_lanes, second_lanes)",
        )
        .unwrap();
        let vector = &arguments.contract.vectors[0];
        assert!(vector.is_repeated());
        assert_eq!(vector.slot, "first_lanes");
        let slots: Vec<String> = vector.slots().iter().map(ToString::to_string).collect();
        assert_eq!(slots, ["first_lanes", "second_lanes"]);

        let plain =
            contract("slots = [instance: U], vectors = [on_uart = instance], constructor = new()")
                .unwrap();
        assert!(!plain.contract.vectors[0].is_repeated());
        assert_eq!(plain.contract.vectors[0].slots().len(), 1);
    }

    #[test]
    fn a_repeated_vector_names_its_slots_once_and_at_least_one() {
        assert!(
            refused("slots = [lanes: L], vectors = [on_lane = [lanes, irq]], constructor = new()")
                .contains("`irq` is not one of this group's `slots`")
        );
        assert!(
            refused(
                "slots = [lanes: L], vectors = [on_lane = [lanes, lanes]], constructor = new()"
            )
            .contains("a slot is listed once")
        );
        assert!(
            refused("slots = [lanes: L], vectors = [on_lane = []], constructor = new()")
                .contains("list the slots")
        );
    }

    #[test]
    fn a_contract_resource_states_its_type() {
        assert!(refused("shared = [uart], constructor = new()").contains("states its type"));
    }

    #[test]
    fn a_contract_resource_has_no_initial_value() {
        assert!(refused("local = [n: u32 = 0], constructor = new()").contains("no initial value"));
    }

    #[test]
    fn a_resource_is_listed_once() {
        assert!(
            refused("shared = [a: u8], local = [a: u8], constructor = new()")
                .contains("duplicate resource")
        );
    }

    #[test]
    fn slots_are_distinct() {
        assert!(refused("slots = [a: U, b: U], constructor = new()").contains("type parameter"));
        assert!(refused("slots = [a: U, a: V], constructor = new()").contains("duplicate slot"));
    }

    #[test]
    fn a_local_is_never_lock_free() {
        assert!(refused("local = [#[lock_free] a: u8], constructor = new()").contains("lock_free"));
    }

    #[test]
    fn an_unknown_argument_names_the_ones_there_are() {
        let error = syn::parse_str::<GroupArguments>("binds = [x]")
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(
            "`slots`, `vectors`, `shared`, `local`, `buffers`, `config` and `constructor`"
        ));
    }

    #[test]
    fn a_constructor_lists_its_arguments() {
        assert!(refused("shared = [a: u8], constructor = new").contains("lists its arguments"));
    }

    #[test]
    fn every_input_is_an_argument_and_every_argument_an_input() {
        assert!(
            refused("buffers = [rx: [u8]], constructor = new(rcc, wiring)")
                .contains("leave out `rx`")
        );
        assert!(
            refused("constructor = new(rcc, frame)").contains("`frame` is not `rcc`, `wiring`")
        );
        assert!(
            refused("slots = [rx: Rx], buffers = [rx: [u8]], constructor = new(rx)")
                .contains("duplicate input")
        );
        assert!(
            refused("slots = [wiring: W], constructor = new(wiring)")
                .contains("the constructor's own argument")
        );
        let arguments =
            contract("slots = [instance: U, cs: Cs], constructor = new(rcc, wiring, cs)").unwrap();
        assert_eq!(arguments.contract.constructor.unwrap().arguments[2], "cs");
        assert!(
            refused("buffers = [rx: [u8]], constructor = new(rx, rx)")
                .contains("each argument once")
        );
        assert!(
            refused("buffers = [rx: [u8]], config = [rx: u32 @ board], constructor = new(rx)")
                .contains("duplicate input")
        );
        assert!(
            refused("config = [rcc: u32 @ board], constructor = new(rcc)")
                .contains("the constructor's own argument")
        );
    }

    #[test]
    fn a_buffer_is_a_slice_of_integers() {
        assert!(
            refused("buffers = [rx: [u8; 64]], constructor = new(rx)")
                .contains("slice of an integer")
        );
        assert!(
            refused("buffers = [rx: [f32]], constructor = new(rx)").contains("slice of an integer")
        );
        assert!(
            refused("buffers = [#[link_section] rx: [u8]], constructor = new(rx)")
                .contains("`#[dma]` and documentation")
        );
        let arguments = contract("buffers = [samples: [u16]], constructor = new(samples)").unwrap();
        assert!(!arguments.contract.buffers[0].dma);
    }

    #[test]
    fn a_config_field_names_its_layer() {
        assert!(
            refused("config = [baud: u32 @ live], constructor = new(baud)")
                .contains("`live` is not an input layer")
        );
        let arguments = contract("config = [mode: Mode @ board], constructor = new(mode)").unwrap();
        assert_eq!(arguments.contract.config[0].layer, Layer::Board);
    }
}
