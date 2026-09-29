//! `app!`: the firmware's authored application, expanded in place into a
//! real `#[rtic::app]`.
//!
//! There is no generated project. Init, `Shared` and `Local` are written here
//! and never move; each task declaration becomes an adapter that constructs the
//! reusable task's own context and calls it.
//!
//! The macro needs nothing from the task crates. Every path it emits is a real
//! Rust path, so a wrong definition, binding or type is an ordinary compile
//! error at the authored line.

use crate::group::{GroupDefinition, GroupInstance};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Attribute, Expr, Ident, Item, LitBool, LitInt, Path, Signature, Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

/// `requirement = resource`, the only genuinely new spelling in the grammar.
/// A bare name binds a requirement to the resource of the same name, as RTIC
/// writes a claim and as Rust writes a struct field it already has in scope.
#[derive(Clone)]
pub(crate) struct Rebind {
    pub(crate) requirement: Ident,
    pub(crate) target: Ident,
}

impl Parse for Rebind {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let requirement: Ident = input.parse()?;
        if !input.peek(Token![=]) {
            return Ok(Self {
                target: requirement.clone(),
                requirement,
            });
        }
        input.parse::<Token![=]>()?;
        Ok(Self {
            requirement,
            target: input.parse()?,
        })
    }
}

/// `name: Type = value`. The type is stated here because an `impl` of the
/// definition's `Config` trait must name it, and the macro never reads the
/// definition. A mismatch fails to compile against the trait.
#[derive(Clone)]
pub(crate) struct ConfigValue {
    pub(crate) docs: Vec<Attribute>,
    pub(crate) name: Ident,
    pub(crate) ty: Type,
    pub(crate) value: Expr,
}

impl Parse for ConfigValue {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let docs = input.call(Attribute::parse_outer)?;
        if let Some(other) = docs
            .iter()
            .find(|attribute| !attribute.path().is_ident("doc"))
        {
            return Err(syn::Error::new(
                other.span(),
                "a configuration value takes documentation and no other attribute",
            ));
        }
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            docs,
            name,
            ty,
            value: input.parse()?,
        })
    }
}

/// One entry of an instance's `local = [..]`: a binding to a firmware resource,
/// or RTIC's own task-local form, `name: Type = value`, which the firmware
/// supplies on the task itself rather than through `#[local]` and `init`.
#[derive(Clone)]
pub(crate) enum LocalBinding {
    Resource(Rebind),
    TaskLocal {
        /// The definition's name for it. The same as `name` when written on an
        /// instance; a group binds a member's requirement to a group-level one.
        requirement: Ident,
        name: Ident,
        ty: Box<Type>,
        value: Box<Expr>,
    },
}

impl Parse for LocalBinding {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Ident) && input.peek2(Token![:]) && !input.peek2(Token![::]) {
            let name: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let ty = Box::new(input.parse()?);
            input.parse::<Token![=]>()?;
            return Ok(Self::TaskLocal {
                requirement: name.clone(),
                name,
                ty,
                value: Box::new(input.parse()?),
            });
        }
        input.parse().map(Self::Resource)
    }
}

impl LocalBinding {
    /// The definition's name for it, and the firmware's.
    pub(crate) fn names(&self) -> (&Ident, &Ident) {
        match self {
            Self::Resource(rebind) => (&rebind.requirement, &rebind.target),
            Self::TaskLocal {
                requirement, name, ..
            } => (requirement, name),
        }
    }

    /// How RTIC claims it: by name, or with its type and initial value.
    fn claim(&self) -> TokenStream {
        match self {
            Self::Resource(rebind) => {
                let target = &rebind.target;
                quote!(#target)
            }
            Self::TaskLocal {
                name, ty, value, ..
            } => quote!(#name: #ty = #value),
        }
    }
}

#[derive(Default)]
pub(crate) struct TaskAttribute {
    pub(crate) from: Option<Path>,
    pub(crate) priority: Option<LitInt>,
    pub(crate) binds: Option<Path>,
    pub(crate) local: Vec<LocalBinding>,
    pub(crate) shared: Vec<Rebind>,
    pub(crate) config: Vec<ConfigValue>,
    pub(crate) spawn: Vec<Rebind>,
}

pub(crate) fn bracketed_list<T: Parse>(input: ParseStream<'_>) -> syn::Result<Vec<T>> {
    let content;
    syn::bracketed!(content in input);
    Ok(Punctuated::<T, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .collect())
}

impl Parse for TaskAttribute {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut parsed = Self::default();
        let mut seen: Vec<String> = Vec::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            // A second `local = [..]` would silently replace the first, so a
            // repeat is refused as a definition's arguments refuse one.
            if seen.contains(&key.to_string()) {
                return Err(syn::Error::new(
                    key.span(),
                    format!("`{key}` appears more than once"),
                ));
            }
            seen.push(key.to_string());
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "from" => parsed.from = Some(input.parse()?),
                "priority" => parsed.priority = Some(input.parse()?),
                "binds" => parsed.binds = Some(input.parse()?),
                "local" => parsed.local = bracketed_list(input)?,
                "shared" => parsed.shared = bracketed_list(input)?,
                "config" => parsed.config = bracketed_list(input)?,
                "spawn" => parsed.spawn = bracketed_list(input)?,
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown task option `{other}`"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(parsed)
    }
}

pub(crate) struct Instance {
    pub(crate) attribute: TaskAttribute,
    /// Every attribute on the declaration but its `#[task(..)]` - documentation,
    /// `#[cfg]` and lints - carried to the task RTIC receives, as RTIC keeps
    /// them on a task of its own.
    pub(crate) attributes: Vec<Attribute>,
    pub(crate) signature: Signature,
}

enum Element {
    /// Authored items - `use`, `Shared`, `Local`, `init` - passed through.
    Verbatim(Item),
    Instance(Instance),
    /// A `#[group(from = ..)] mod name;` selection of several tasks at once.
    Group(GroupInstance),
    /// What a group's own macro appended: its members, keyed by the selection
    /// they are for. Never authored.
    Definition(GroupDefinition),
}

/// A bodyless `#[task(from = ..)] fn name(..);` declaration.
fn parse_instance(
    input: ParseStream<'_>,
    attribute: TaskAttribute,
    attributes: Vec<Attribute>,
) -> syn::Result<Instance> {
    let signature: Signature = input.parse()?;
    input.parse::<Token![;]>()?;
    let attributes = attributes
        .into_iter()
        .filter(|attribute| !attribute.path().is_ident("task"))
        .collect();
    Ok(Instance {
        attribute,
        attributes,
        signature,
    })
}

/// The `#[task(..)]` attribute on an item, when it declares an instance rather
/// than being RTIC's own.
///
/// An attribute that names `from = ..` is an instance whatever else it says, so
/// its parse error is the author's to see. Only one that does not is left for
/// RTIC, whose own options - `local = [x: u32 = 0]` among them - this grammar
/// does not read.
fn instance_attribute(attributes: &[Attribute]) -> syn::Result<Option<TaskAttribute>> {
    let Some(attribute) = attributes
        .iter()
        .find(|attribute| attribute.path().is_ident("task"))
    else {
        return Ok(None);
    };
    match attribute.parse_args_with(TaskAttribute::parse) {
        Ok(parsed) => Ok(parsed.from.is_some().then_some(parsed)),
        Err(error) if names_a_definition(attribute) => Err(error),
        Err(_) => Ok(None),
    }
}

/// `from =` at the top level of the attribute's arguments.
fn names_a_definition(attribute: &Attribute) -> bool {
    let syn::Meta::List(list) = &attribute.meta else {
        return false;
    };
    let tokens = list.tokens.clone().into_iter().collect::<Vec<_>>();
    tokens.windows(2).any(|pair| {
        matches!(&pair[0], proc_macro2::TokenTree::Ident(ident) if ident == "from")
            && matches!(&pair[1], proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '=')
    })
}

/// The type name substituted for a task's monotonic slot. An alias rather than
/// the author's own name, so an application that declares no monotonic still
/// has something to put in the slot.
///
/// The name comes from the declaration itself: every `rtic-monotonics` macro is
/// `<timer>_monotonic!(Name, ..)`, written inside the application as in ordinary
/// RTIC, so there is nothing for the header to restate.
const MONOTONIC_SLOT: &str = "__FfMonotonic";

/// What the slot resolves to when nothing was declared. Named for what it means,
/// because a task that does need a clock fails against this name.
const NO_MONOTONIC: &str = "NoMonotonicDeclared";

pub struct App {
    /// Everything `app!` was given, kept so that a group's macro can be handed
    /// the whole application back. See [the group module](mod@crate::group).
    original: TokenStream,
    device: Path,
    dispatchers: Vec<Ident>,
    /// `None` leaves RTIC's own default alone rather than restating it.
    peripherals: Option<LitBool>,
    elements: Vec<Element>,
}

/// Header arguments, parsed as RTIC parses its own: a loop, so order does not
/// matter, with defaults for everything a firmware need not say.
impl Parse for App {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let original: TokenStream = input.fork().parse()?;
        let mut device: Option<Path> = None;
        let mut dispatchers = Vec::new();
        let mut peripherals = None;
        let mut seen: Vec<String> = Vec::new();

        // A header entry is `ident = ...`. Items begin with `#`, or a keyword
        // such as `use`, `struct` or `fn`, and a macro call has `!` where this
        // wants `=` - so this never mistakes one for the other.
        while input.peek(Ident) && input.peek2(Token![=]) {
            let key: Ident = input.parse()?;
            let name = key.to_string();
            if seen.contains(&name) {
                return Err(syn::Error::new(
                    key.span(),
                    format!("`{name}` appears more than once"),
                ));
            }
            seen.push(name.clone());
            input.parse::<Token![=]>()?;

            match name.as_str() {
                "device" => device = Some(input.parse()?),
                "dispatchers" => dispatchers = bracketed_list(input)?,
                "peripherals" => peripherals = Some(input.parse()?),
                // It was one, and RTIC never had it. Saying where it went is
                // cheaper than leaving the author to find out.
                "monotonic" => {
                    return Err(syn::Error::new(
                        key.span(),
                        "`monotonic` is not an argument: declare the monotonic inside \
                         `app!`, as in RTIC, e.g. `systick_monotonic!(Mono, 1000);`",
                    ));
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown argument `{other}`; expected `device`, \
                             `dispatchers` or `peripherals`"
                        ),
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let device = device.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "an application needs `device = <path to the PAC>`",
            )
        })?;

        let mut elements = Vec::new();
        while !input.is_empty() {
            if input.peek(Token![@]) {
                elements.push(Element::Definition(input.parse()?));
                continue;
            }
            let attributes = input.call(Attribute::parse_outer)?;
            if attributes
                .iter()
                .any(|attribute| attribute.path().is_ident("group"))
            {
                elements.push(Element::Group(GroupInstance::parse_with(
                    input, attributes,
                )?));
            } else if let Some(attribute) = instance_attribute(&attributes)? {
                elements.push(Element::Instance(parse_instance(
                    input, attribute, attributes,
                )?));
            } else {
                let mut item: Item = input.parse()?;
                prepend_attributes(&mut item, attributes)?;
                elements.push(Element::Verbatim(item));
            }
        }

        Ok(Self {
            original,
            device,
            dispatchers,
            peripherals,
            elements,
        })
    }
}

/// The type a `<timer>_monotonic!(Name, ..)` item declares, if it is one.
fn declared_monotonic(item: &Item) -> syn::Result<Option<Ident>> {
    let Item::Macro(item) = item else {
        return Ok(None);
    };
    let is_monotonic = item
        .mac
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident.to_string().ends_with("_monotonic"));
    if !is_monotonic {
        return Ok(None);
    }
    item.mac
        .parse_body_with(|input: ParseStream<'_>| {
            let name = input.parse::<Ident>()?;
            input.parse::<TokenStream>()?;
            Ok(name)
        })
        .map(Some)
}

/// The application's monotonic. Tasks are handed one clock type, so two
/// declarations would leave the adapters with a choice only the author can
/// make; that is refused at the second rather than resolved by position.
fn application_monotonic(elements: &[Element]) -> syn::Result<Option<Ident>> {
    let mut found: Option<Ident> = None;
    for element in elements {
        let Element::Verbatim(item) = element else {
            continue;
        };
        let Some(name) = declared_monotonic(item)? else {
            continue;
        };
        if let Some(first) = &found {
            return Err(syn::Error::new(
                name.span(),
                format!(
                    "a second monotonic; tasks are handed one clock, and `{first}` is \
                     already declared"
                ),
            ));
        }
        found = Some(name);
    }
    Ok(found)
}

/// Put back the attributes read ahead of an item that turned out not to be an
/// instance. Every item RTIC accepts inside its module is passed through, as
/// RTIC passes anything it does not recognise through as the author's code.
fn prepend_attributes(item: &mut Item, mut attributes: Vec<Attribute>) -> syn::Result<()> {
    let existing = match item {
        Item::Use(item) => &mut item.attrs,
        Item::Struct(item) => &mut item.attrs,
        Item::Enum(item) => &mut item.attrs,
        Item::Union(item) => &mut item.attrs,
        Item::Fn(item) => &mut item.attrs,
        Item::Impl(item) => &mut item.attrs,
        Item::Trait(item) => &mut item.attrs,
        Item::TraitAlias(item) => &mut item.attrs,
        Item::Const(item) => &mut item.attrs,
        Item::Static(item) => &mut item.attrs,
        Item::Type(item) => &mut item.attrs,
        Item::Mod(item) => &mut item.attrs,
        Item::Macro(item) => &mut item.attrs,
        Item::ExternCrate(item) => &mut item.attrs,
        Item::ForeignMod(item) => &mut item.attrs,
        // Tokens syn could not classify carry no attribute list of their own,
        // so any read ahead of them would be lost rather than passed on.
        _ if attributes.is_empty() => return Ok(()),
        other => {
            return Err(syn::Error::new(
                other.span(),
                "attributes on an item `app!` cannot pass through",
            ));
        }
    };
    attributes.append(existing);
    *existing = attributes;
    Ok(())
}

pub(crate) fn render_instance(instance: &Instance) -> syn::Result<TokenStream> {
    let Instance {
        attribute,
        attributes,
        signature,
    } = instance;
    // A gated instance takes its configuration with it, or the configuration
    // would outlive the task it was for and be reported as unused.
    let cfgs = attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("cfg"))
        .collect::<Vec<_>>();
    let name = &signature.ident;
    let from = attribute
        .from
        .as_ref()
        .expect("instances are recognised by their `from`");

    // The adapter reads the context by the name the declaration gives it, so
    // `_cx` works as well as `cx`.
    let context = signature
        .inputs
        .first()
        .ok_or_else(|| syn::Error::new(signature.span(), "task needs a context parameter"))?;
    let cx = match context {
        syn::FnArg::Typed(typed) => match typed.pat.as_ref() {
            syn::Pat::Ident(pattern) => pattern.ident.clone(),
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "name the context, such as `cx`; the adapter reads it",
                ));
            }
        },
        other => return Err(syn::Error::new(other.span(), "expected a task context")),
    };

    // Every piece of the adapter carries the span of the authored binding it
    // came from, so a type error in it is reported on that line of the
    // declaration rather than at `ferroforge::app! {`.
    let local_fields = attribute.local.iter().map(|binding| {
        let (requirement, resource) = binding.names();
        quote_spanned!(resource.span()=> #requirement: #cx.local.#resource)
    });
    let shared = &attribute.shared;
    let shared_fields = shared.iter().map(|rebind| {
        let (requirement, resource) = (&rebind.requirement, &rebind.target);
        quote_spanned!(resource.span()=> #requirement: #cx.shared.#resource)
    });
    // The definition's own state, as one RTIC task-local. Declared on every
    // instance, because the macro does not read the definition to know whether
    // it has any; with none, the struct is empty and costs nothing.
    let task_local = format_ident!("__ff_task_{}", name);
    let local_claims = attribute.local.iter().map(LocalBinding::claim);
    let local_attribute = quote!(
        , local = [#(#local_claims,)* #task_local: #from::__FfTaskLocal = #from::__FfTaskLocal::INIT]
    );
    let shared_claims = shared.iter().map(|rebind| &rebind.target);
    let shared_attribute = (!shared.is_empty()).then(|| quote!(, shared = [#(#shared_claims),*]));

    // The bound instance's real RTIC `spawn` function, not a closure around
    // it. A function item satisfies the definition's
    // `Fn(A, B) -> Result<(), (A, B)>` for any number of inputs, so this needs
    // nothing from the definition; a one-argument closure fit only one.
    let spawn_fields = attribute.spawn.iter().map(|rebind| {
        let (alias, target) = (&rebind.requirement, &rebind.target);
        quote_spanned!(target.span()=> #alias: #target::spawn)
    });

    let config_type = format_ident!("__FfConfig{}", name.to_string().to_uppercase());
    let monotonic_slot = format_ident!("{MONOTONIC_SLOT}");
    let config_consts = attribute.config.iter().map(|value| {
        let (docs, name, ty, value) = (&value.docs, &value.name, &value.ty, &value.value);
        let name = format_ident!("{}", name.to_string().to_uppercase(), span = name.span());
        quote_spanned!(name.span()=> #(#docs)* const #name: #ty = #value;)
    });

    let priority = attribute
        .priority
        .clone()
        .unwrap_or_else(|| LitInt::new("1", name.span()));
    // Per G3 the binding is what makes a task a hardware task, and RTIC runs a
    // bound handler synchronously. The contradiction is visible in the authored
    // declaration alone, so it is caught here rather than surfacing later as
    // RTIC's generic complaint about the handler's signature.
    if let (Some(asyncness), Some(interrupt)) = (&signature.asyncness, &attribute.binds) {
        let interrupt = interrupt
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default();
        return Err(syn::Error::new(
            asyncness.span,
            format!(
                "an `async` task cannot bind an interrupt: `binds = {interrupt}` makes \
                 `{name}` a hardware task. Drop `async` and select a hardware definition, \
                 or remove `binds` to leave it a software task"
            ),
        ));
    }

    let binds = attribute
        .binds
        .as_ref()
        .map(|interrupt| quote!(binds = #interrupt,));

    let inputs = signature.inputs.iter().skip(1).collect::<Vec<_>>();
    let forwarded = signature
        .inputs
        .iter()
        .skip(1)
        .map(|argument| match argument {
            syn::FnArg::Typed(typed) => Ok(typed.pat.clone()),
            other => Err(syn::Error::new(other.span(), "unsupported task input")),
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let asyncness = &signature.asyncness;
    let awaiting = signature.asyncness.map(|_| quote!(.await));
    let output = &signature.output;

    // Checked once per instance, where the definition is named, so a monotonic
    // the definition cannot use is reported with the definition's own reason.
    let slot_at_from = format_ident!("{MONOTONIC_SLOT}", span = from.span());
    let check = quote_spanned! {from.span()=>
        const _: fn() = #from::__ff_check_monotonic::<#slot_at_from>;
    };

    let call = quote_spanned! {from.span()=>
        #from(
            #from::Context {
                local: #from::__FfBound {
                    __ff_task: #cx.local.#task_local,
                    #(#local_fields),*
                }.into_local(),
                shared: #from::Shared { #(#shared_fields),* },
                spawn: #from::Spawn { #(#spawn_fields),* },
                config: ::core::marker::PhantomData::<#config_type>,
                monotonic: ::core::marker::PhantomData::<#monotonic_slot>,
            }
            #(, #forwarded)*
        ) #awaiting
    };

    Ok(quote! {
        #(#cfgs)*
        struct #config_type;
        #(#cfgs)*
        impl #from::Config for #config_type {
            #(#config_consts)*
        }
        #(#cfgs)*
        #check

        #(#attributes)*
        #[task(#binds priority = #priority #local_attribute #shared_attribute)]
        #asyncness fn #name(#context #(, #inputs)*) #output {
            #call
        }
    })
}

pub fn expand(application: App) -> syn::Result<TokenStream> {
    let App {
        original,
        device,
        dispatchers,
        peripherals,
        elements,
    } = application;
    let monotonic = application_monotonic(&elements)?;

    // A group selected but not yet defined here is handed to its own macro,
    // which appends its members and calls `app!` again. One group per round.
    let definitions = elements
        .iter()
        .filter_map(|element| match element {
            Element::Definition(definition) => Some(definition),
            _ => None,
        })
        .collect::<Vec<_>>();
    let uses = elements
        .iter()
        .filter_map(|element| match element {
            Element::Verbatim(Item::Use(item)) => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut groups: Vec<&Ident> = Vec::new();
    for element in &elements {
        let Element::Group(group) = element else {
            continue;
        };
        if let Some(first) = groups.iter().find(|name| **name == &group.name) {
            return Err(syn::Error::new(
                group.name.span(),
                format!("a second group named `{first}`"),
            ));
        }
        groups.push(&group.name);
        if let Some(callback) = crate::group::next_callback(group, &definitions, &uses, &original) {
            return Ok(callback);
        }
    }

    let mut body = Vec::new();
    for element in &elements {
        body.push(match element {
            Element::Verbatim(item) => quote!(#item),
            Element::Instance(instance) => render_instance(instance)?,
            Element::Group(group) => {
                let mine = crate::group::definitions_for(group, &definitions);
                let (instances, module) = crate::group::instances(group, &mine, &uses)?;
                let mut rendered = TokenStream::new();
                for instance in instances {
                    rendered.extend(render_instance(&instance)?);
                }
                rendered.extend(module);
                rendered
            }
            Element::Definition(_) => TokenStream::new(),
        });
    }

    // Passed through only when stated, so RTIC's own default stands otherwise
    // rather than being restated here and drifting from it.
    let peripherals = peripherals.map(|value| quote!(, peripherals = #value));
    let dispatchers =
        (!dispatchers.is_empty()).then(|| quote!(, dispatchers = [#(#dispatchers),*]));

    // Every task context carries a monotonic slot, so the slot always needs a
    // type. An application that declared one aliases it; one that did not gets
    // an uninhabited stand-in, and a task that actually needs a clock then fails
    // against a name that says why. The declaration itself stays where the
    // author wrote it, among the items, so `init` and tasks name it directly.
    let slot = format_ident!("{MONOTONIC_SLOT}");
    let monotonic = match monotonic {
        Some(name) => quote! {
            type #slot = #name;
        },
        None => {
            let absent = format_ident!("{NO_MONOTONIC}");
            quote! {
                enum #absent {}
                type #slot = #absent;
            }
        }
    };

    // The expansion contains another attribute macro; expansion is recursive,
    // so `#[rtic::app]` runs on the result of this one.
    Ok(quote! {
        #[rtic::app(device = #device #dispatchers #peripherals)]
        mod app {
            #monotonic
            #(#body)*
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_source(tasks: &str) -> syn::Result<TokenStream> {
        let source = format!("device = chip::pac, dispatchers = [SPARE],\n{tasks}");
        expand(syn::parse_str::<App>(&source)?)
    }

    /// Nothing here reads the task crate, so the rejection has to come from the
    /// authored declaration alone.
    #[test]
    fn rejects_an_async_task_bound_to_an_interrupt() {
        let error =
            expand_source("#[task(from = blink, binds = TIM2)] async fn led(cx: led::Context);")
                .expect_err("an async task binding an interrupt must be rejected")
                .to_string();
        assert!(error.contains("cannot bind an interrupt"), "{error}");
        assert!(
            error.contains("TIM2"),
            "the interrupt must be named: {error}"
        );
        assert!(error.contains("led"), "the instance must be named: {error}");
    }

    /// An attribute naming a definition is an instance, so a mistake in the
    /// rest of it is reported as that mistake - not dropped, leaving the
    /// bodyless declaration to fail later as a confusing item parse.
    #[test]
    fn a_bad_option_on_an_instance_is_reported() {
        let error = match syn::parse_str::<App>(
            "device = chip::pac, #[task(from = blink, typo = y)] async fn led(cx: led::Context);",
        ) {
            Ok(_) => panic!("an unknown option must be refused"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("unknown task option `typo`"), "{error}");
    }

    /// A configuration value's documentation reaches the constant it becomes,
    /// and any other attribute is refused rather than dropped.
    #[test]
    fn a_configuration_value_takes_documentation_only() {
        let output = expand_source(
            "#[task(from = blink, config = [/// Half a second.\n period_ms: u32 = 500])] \
             async fn led(cx: led::Context);",
        )
        .unwrap()
        .to_string();
        assert!(
            output.contains("doc = \" Half a second.\"] const PERIOD_MS"),
            "{output}"
        );

        let error = match syn::parse_str::<App>(
            "device = chip::pac, #[task(from = blink, config = [#[inline] period_ms: u32 = 500])] \
             async fn led(cx: led::Context);",
        ) {
            Ok(_) => panic!("an attribute other than documentation must be refused"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("documentation and no other"), "{error}");
    }

    /// An instance's other attributes reach the task RTIC receives, and a
    /// `#[cfg]` also gates the configuration made for it, so an instance gated
    /// off leaves nothing behind.
    #[test]
    fn an_instances_attributes_reach_its_task() {
        let output = expand_source(
            "/// The status light.\n#[cfg(feature = \"led\")]\n\
             #[task(from = blink)] async fn led(cx: led::Context);",
        )
        .unwrap()
        .to_string();
        assert!(output.contains("doc = \" The status light.\""), "{output}");
        let gate = "# [cfg (feature = \"led\")]";
        // The configuration's struct and impl, the monotonic check, and the task.
        assert_eq!(output.matches(gate).count(), 4, "{output}");
    }

    /// A repeated option would silently replace the first, and options run
    /// together are a typo; both are refused as a definition refuses them.
    #[test]
    fn an_instances_options_are_separated_and_given_once() {
        for (source, message) in [
            (
                "#[task(from = blink, local = [a], local = [b])] async fn led(cx: led::Context);",
                "`local` appears more than once",
            ),
            (
                "#[task(from = blink priority = 1)] async fn led(cx: led::Context);",
                "expected `,`",
            ),
        ] {
            let error = match syn::parse_str::<App>(&format!("device = chip::pac, {source}")) {
                Ok(_) => panic!("{source} must be refused"),
                Err(error) => error.to_string(),
            };
            assert!(error.contains(message), "{source}: {error}");
        }
    }

    /// RTIC passes any item it does not recognise through as the author's
    /// code, so `app!` does too.
    #[test]
    fn any_item_rtic_accepts_passes_through() {
        let output = expand_source(
            "#[derive(Clone, Copy)] enum Mode { A } static LIMIT: u32 = 3; trait Named {}",
        )
        .unwrap()
        .to_string();
        assert!(output.contains("enum Mode"), "{output}");
        assert!(output.contains("static LIMIT"), "{output}");
        assert!(output.contains("trait Named"), "{output}");
        assert!(output.contains("derive (Clone , Copy)"), "{output}");
    }

    /// Each instance is checked against its definition's own monotonic check,
    /// which is what explains a monotonic counting in the wrong width.
    #[test]
    fn each_instance_is_checked_against_its_definitions_monotonic() {
        let output = expand_source("#[task(from = blink)] async fn led(cx: led::Context);")
            .unwrap()
            .to_string();
        assert!(
            output.contains("blink :: __ff_check_monotonic :: < __FfMonotonic >"),
            "{output}"
        );
    }

    /// RTIC's own task options are not this grammar's, and a task without
    /// `from` is RTIC's to read.
    #[test]
    fn an_rtic_task_is_left_to_rtic() {
        expand_source(
            "#[task(priority = 1, local = [n: u32 = 0])] async fn own(cx: own::Context) {}",
        )
        .expect("RTIC's own local initializers must pass through");
    }

    /// A bare name binds to the resource of the same name, as RTIC writes it.
    #[test]
    fn a_bare_binding_names_the_resource_of_the_same_name() {
        let output = expand_source(
            "#[task(from = blink, local = [led, count = blink_count], shared = [enabled])] \
             async fn status(cx: status::Context);",
        )
        .unwrap()
        .to_string();
        assert!(output.contains("led : cx . local . led"), "{output}");
        assert!(
            output.contains("count : cx . local . blink_count"),
            "{output}"
        );
        assert!(
            output.contains("enabled : cx . shared . enabled"),
            "{output}"
        );
    }

    /// A firmware may supply a local on the task itself, as RTIC writes a
    /// task-local: passed to RTIC as written, and bound by its own name.
    #[test]
    fn an_instance_may_supply_a_local_as_an_rtic_task_local() {
        let output = expand_source(
            "#[task(from = blink, local = [led, detector: Detector = Detector::new(3, 4)])] \
             async fn status(cx: status::Context);",
        )
        .unwrap()
        .to_string();
        assert!(
            output.contains("detector : Detector = Detector :: new (3 , 4)"),
            "{output}"
        );
        assert!(
            output.contains("detector : cx . local . detector"),
            "{output}"
        );
        assert!(output.contains("led : cx . local . led"), "{output}");
    }

    /// RTIC's `spawn` function itself, which fits any number of inputs.
    #[test]
    fn a_spawn_alias_is_the_targets_spawn_function() {
        let output = expand_source(
            "#[task(from = blink, spawn = [report = telemetry])] async fn led(cx: led::Context);",
        )
        .unwrap()
        .to_string();
        assert!(output.contains("report : telemetry :: spawn"), "{output}");
        assert!(!output.contains("| value |"), "{output}");
    }

    /// The control: the same binding on a synchronous task is an ordinary
    /// hardware task and must pass through.
    #[test]
    fn accepts_a_synchronous_task_bound_to_an_interrupt() {
        expand_source("#[task(from = on_tick, binds = TIM2)] fn tick(cx: tick::Context);")
            .expect("a synchronous bound task is a hardware task");
    }
}

/// The header, which RTIC parses as a loop with defaults. These assert that
/// `app!` does the same, because the previous fixed-order form reported a
/// swapped argument as "expected `device`" - blaming the wrong one.
#[cfg(test)]
mod header {
    use super::*;

    fn parse(header: &str) -> syn::Result<App> {
        syn::parse_str::<App>(header)
    }

    fn rendered(header: &str) -> String {
        match parse(header) {
            Ok(application) => expand(application).unwrap().to_string(),
            Err(error) => panic!("`{header}` must parse: {error}"),
        }
    }

    fn refused(header: &str) -> String {
        match parse(header) {
            Ok(_) => panic!("`{header}` must not parse"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn arguments_may_appear_in_any_order() {
        assert!(
            parse("dispatchers = [A], peripherals = true, device = chip::pac,").is_ok(),
            "order must not matter, as in RTIC"
        );
    }

    /// RTIC defaults `dispatchers` to empty. An application with no software
    /// tasks has nothing to dispatch and should not have to say so.
    #[test]
    fn dispatchers_are_optional_and_omitted_when_empty() {
        let output = rendered("device = chip::pac,");
        assert!(!output.contains("dispatchers"), "{output}");
    }

    /// Passed through only when stated, so RTIC's own default is never restated
    /// here where it could drift from RTIC's.
    #[test]
    fn peripherals_is_passed_through_only_when_stated() {
        assert!(!rendered("device = chip::pac,").contains("peripherals"));
        let stated = rendered("device = chip::pac, peripherals = false,");
        assert!(stated.contains("peripherals = false"), "{stated}");
    }

    /// The one argument an application cannot do without.
    #[test]
    fn device_is_required_and_says_so() {
        let error = refused("dispatchers = [A],");
        assert!(error.contains("device"), "{error}");
    }

    #[test]
    fn a_repeated_argument_is_named() {
        let error = refused("device = a::pac, device = b::pac,");
        assert!(error.contains("more than once"), "{error}");
        assert!(error.contains("device"), "{error}");
    }

    #[test]
    fn an_unknown_argument_lists_the_real_ones() {
        let error = refused("device = chip::pac, monotonic_hz = 1000,");
        assert!(error.contains("monotonic_hz"), "{error}");
        assert!(error.contains("dispatchers"), "{error}");
        assert!(error.contains("peripherals"), "{error}");
    }

    /// The header argument it used to be must say where the declaration went.
    #[test]
    fn monotonic_in_the_header_points_at_the_declaration() {
        let error = refused("device = chip::pac, monotonic = Mono,");
        assert!(error.contains("systick_monotonic!"), "{error}");
    }

    /// An application without a monotonic still has a slot to fill, so it gets
    /// a stand-in named for what it means.
    #[test]
    fn an_application_without_a_monotonic_still_expands() {
        let output = rendered("device = chip::pac,");
        assert!(output.contains(NO_MONOTONIC), "{output}");
    }

    /// Declared inside, as in RTIC: the declaration stays among the items, and
    /// its type fills the slot.
    #[test]
    fn a_monotonic_declared_inside_fills_the_slot() {
        let output = rendered("device = chip::pac, systick_monotonic!(Mono, 1000);");
        assert!(
            output.contains("systick_monotonic ! (Mono , 1000)"),
            "{output}"
        );
        assert!(output.contains("type __FfMonotonic = Mono"), "{output}");
        assert!(!output.contains(NO_MONOTONIC), "{output}");
    }

    /// Any timer's macro, by any path: they all share the `_monotonic` suffix.
    #[test]
    fn any_rtic_monotonics_macro_is_recognized() {
        let output = rendered(
            "device = chip::pac, rtic_monotonics::stm32_tim2_monotonic!(Clock, 1_000_000);",
        );
        assert!(output.contains("type __FfMonotonic = Clock"), "{output}");
    }

    /// Other item macros are the author's and pass through untouched.
    #[test]
    fn other_item_macros_are_not_monotonics() {
        let output = rendered("device = chip::pac, some_macro!(Thing);");
        assert!(output.contains("some_macro ! (Thing)"), "{output}");
        assert!(output.contains(NO_MONOTONIC), "{output}");
    }

    #[test]
    fn a_second_monotonic_is_refused() {
        let application = parse(
            "device = chip::pac, systick_monotonic!(A, 1000); \
             stm32_tim2_monotonic!(B, 1000);",
        )
        .unwrap();
        let error = expand(application).unwrap_err().to_string();
        assert!(error.contains("second monotonic"), "{error}");
        assert!(error.contains('A'), "{error}");
    }
}
