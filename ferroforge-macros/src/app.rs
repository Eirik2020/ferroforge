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

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, Expr, Ident, Item, LitBool, LitInt, Path, Signature, Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

/// `requirement = resource`, the only genuinely new spelling in the grammar.
struct Rebind {
    requirement: Ident,
    target: Ident,
}

impl Parse for Rebind {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let requirement = input.parse()?;
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
struct ConfigValue {
    name: Ident,
    ty: Type,
    value: Expr,
}

impl Parse for ConfigValue {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            name,
            ty,
            value: input.parse()?,
        })
    }
}

#[derive(Default)]
struct TaskAttribute {
    from: Option<Path>,
    priority: Option<LitInt>,
    binds: Option<Path>,
    local: Vec<Rebind>,
    shared: Vec<Rebind>,
    config: Vec<ConfigValue>,
    spawn: Vec<Rebind>,
}

fn bracketed_list<T: Parse>(input: ParseStream<'_>) -> syn::Result<Vec<T>> {
    let content;
    syn::bracketed!(content in input);
    Ok(Punctuated::<T, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .collect())
}

impl Parse for TaskAttribute {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut parsed = Self::default();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
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
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(parsed)
    }
}

struct Instance {
    attribute: TaskAttribute,
    signature: Signature,
}

enum Element {
    /// Authored items - `use`, `Shared`, `Local`, `init` - passed through.
    Verbatim(Item),
    Instance(Instance),
}

/// The type name substituted for a task's monotonic slot. An alias rather than
/// the author's own name, so an application that declares no monotonic still
/// has something to put in the slot.
const MONOTONIC_SLOT: &str = "__FfMonotonic";

/// What the slot resolves to when nothing was declared. Named for what it means,
/// because a task that does need a clock fails against this name.
const NO_MONOTONIC: &str = "NoMonotonicDeclared";

pub struct App {
    device: Path,
    dispatchers: Vec<Ident>,
    /// `None` leaves RTIC's own default alone rather than restating it.
    peripherals: Option<LitBool>,
    /// The application's monotonic, declared outside this macro as in ordinary
    /// RTIC and named here so adapters can hand it to tasks.
    monotonic: Option<Ident>,
    elements: Vec<Element>,
}

/// Header arguments, parsed as RTIC parses its own: a loop, so order does not
/// matter, with defaults for everything a firmware need not say.
impl Parse for App {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut device: Option<Path> = None;
        let mut dispatchers = Vec::new();
        let mut peripherals = None;
        let mut monotonic = None;
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
                "monotonic" => monotonic = Some(input.parse()?),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown argument `{other}`; expected `device`, \
                             `dispatchers`, `peripherals` or `monotonic`"
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
            let attributes = input.call(Attribute::parse_outer)?;
            let task = attributes
                .iter()
                .find(|attribute| attribute.path().is_ident("task"));
            let declares_instance = task
                .map(|attribute| {
                    attribute
                        .parse_args_with(TaskAttribute::parse)
                        .map(|parsed| parsed.from.is_some())
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if declares_instance {
                let attribute = task
                    .expect("checked above")
                    .parse_args_with(TaskAttribute::parse)?;
                let signature: Signature = input.parse()?;
                input.parse::<Token![;]>()?;
                elements.push(Element::Instance(Instance {
                    attribute,
                    signature,
                }));
            } else {
                let mut item: Item = input.parse()?;
                prepend_attributes(&mut item, attributes)?;
                elements.push(Element::Verbatim(item));
            }
        }

        Ok(Self {
            device,
            dispatchers,
            peripherals,
            monotonic,
            elements,
        })
    }
}

fn prepend_attributes(item: &mut Item, mut attributes: Vec<Attribute>) -> syn::Result<()> {
    let existing = match item {
        Item::Use(item) => &mut item.attrs,
        Item::Struct(item) => &mut item.attrs,
        Item::Fn(item) => &mut item.attrs,
        Item::Impl(item) => &mut item.attrs,
        Item::Const(item) => &mut item.attrs,
        Item::Type(item) => &mut item.attrs,
        Item::Mod(item) => &mut item.attrs,
        other => {
            return Err(syn::Error::new(
                other.span(),
                "unsupported item in a composition",
            ));
        }
    };
    attributes.append(existing);
    *existing = attributes;
    Ok(())
}

fn render_instance(instance: &Instance) -> syn::Result<TokenStream> {
    let Instance {
        attribute,
        signature,
    } = instance;
    let name = &signature.ident;
    let from = attribute
        .from
        .as_ref()
        .expect("instances are recognised by their `from`");

    let local_fields = attribute.local.iter().map(|rebind| {
        let (requirement, resource) = (&rebind.requirement, &rebind.target);
        quote!(#requirement: cx.local.#resource)
    });
    let shared_fields = attribute.shared.iter().map(|rebind| {
        let (requirement, resource) = (&rebind.requirement, &rebind.target);
        quote!(#requirement: cx.shared.#resource)
    });
    let local_claims = attribute.local.iter().map(|rebind| &rebind.target);
    let shared_claims = attribute.shared.iter().map(|rebind| &rebind.target);
    let local_attribute =
        (!attribute.local.is_empty()).then(|| quote!(, local = [#(#local_claims),*]));
    let shared_attribute =
        (!attribute.shared.is_empty()).then(|| quote!(, shared = [#(#shared_claims),*]));

    // The spawn closure forwards to the bound instance's real RTIC `spawn`.
    // Argument count comes from the definition, so the closure takes the
    // arguments it is given rather than naming them.
    let spawn_fields = attribute.spawn.iter().map(|rebind| {
        let (alias, target) = (&rebind.requirement, &rebind.target);
        quote!(#alias: |value| #target::spawn(value))
    });

    let config_type = format_ident!("__FfConfig{}", name.to_string().to_uppercase());
    let monotonic_slot = format_ident!("{MONOTONIC_SLOT}");
    let config_consts = attribute.config.iter().map(|value| {
        let (name, ty, value) = (&value.name, &value.ty, &value.value);
        let name = format_ident!("{}", name.to_string().to_uppercase(), span = name.span());
        quote!(const #name: #ty = #value;)
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
    let context = signature
        .inputs
        .first()
        .ok_or_else(|| syn::Error::new(signature.span(), "task needs a context parameter"))?;
    let asyncness = &signature.asyncness;
    let awaiting = signature.asyncness.map(|_| quote!(.await));
    let output = &signature.output;

    Ok(quote! {
        struct #config_type;
        impl #from::Config for #config_type {
            #(#config_consts)*
        }

        #[task(#binds priority = #priority #local_attribute #shared_attribute)]
        #asyncness fn #name(#context #(, #inputs)*) #output {
            #from(
                #from::Context {
                    local: #from::Local { #(#local_fields),* },
                    shared: #from::Shared { #(#shared_fields),* },
                    spawn: #from::Spawn { #(#spawn_fields),* },
                    config: ::core::marker::PhantomData::<#config_type>,
                    monotonic: ::core::marker::PhantomData::<#monotonic_slot>,
                }
                #(, #forwarded)*
            ) #awaiting
        }
    })
}

pub fn expand(application: App) -> syn::Result<TokenStream> {
    let App {
        device,
        dispatchers,
        peripherals,
        monotonic,
        elements,
    } = application;

    let mut body = Vec::new();
    for element in &elements {
        body.push(match element {
            Element::Verbatim(item) => quote!(#item),
            Element::Instance(instance) => render_instance(instance)?,
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
    // against a name that says why.
    let slot = format_ident!("{MONOTONIC_SLOT}");
    let monotonic = match monotonic {
        // Imported under the author's own name as well, because `init` starts it
        // and tasks may use it directly, exactly as in ordinary RTIC.
        Some(name) => quote! {
            use super::#name;
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
        let source =
            format!("device = chip::pac, dispatchers = [SPARE], monotonic = Mono,\n{tasks}");
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
            parse("dispatchers = [A], monotonic = Mono, device = chip::pac,").is_ok(),
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
        assert!(error.contains("monotonic"), "{error}");
    }

    /// An application without a monotonic still has a slot to fill, so it gets
    /// a stand-in named for what it means.
    #[test]
    fn an_application_without_a_monotonic_still_expands() {
        let output = rendered("device = chip::pac,");
        assert!(output.contains(NO_MONOTONIC), "{output}");
    }

    /// With one, the author's own name is in scope too - `init` starts it.
    #[test]
    fn a_declared_monotonic_is_imported_under_its_own_name() {
        let output = rendered("device = chip::pac, monotonic = Mono,");
        assert!(output.contains("use super :: Mono"), "{output}");
        assert!(!output.contains(NO_MONOTONIC), "{output}");
    }
}
