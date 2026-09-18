//! Source contracts shared by the procedural macro and host discovery.
//!
//! This crate parses declarations, not task-body semantics. It has no target
//! HAL dependencies. Parsing the standalone syntax is not a checking expansion.

use std::collections::BTreeSet;

use syn::{
    Error, Expr, FnArg, Ident, LitStr, Pat, Path, ReturnType, Signature, Token, Type,
    TypeParamBound, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

/// Rust raw and ordinary identifiers name the same declaration.
pub fn identifier_key(name: &Ident) -> String {
    name.to_string().trim_start_matches("r#").to_owned()
}

/// An inline resource or configuration entry. A resource needs either a type or
/// a matching bound; `None` is the bare-name form, which `#[task]` rejects for
/// resources because it cannot infer a field type from a name alone.
///
/// A local resource may also carry an initial value, `name: Type = expr`, as
/// in RTIC. It is then the task's own state: the definition supplies it, and a
/// firmware selecting the task binds nothing for it.
#[derive(Clone, Debug)]
pub struct Resource {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Option<Expr>,
}

impl Parse for Resource {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        let ty = if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        let init = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self { name, ty, init })
    }
}

#[derive(Clone, Debug)]
pub struct ResourceBound {
    pub name: Ident,
    pub traits: Punctuated<TypeParamBound, Token![+]>,
}

impl Parse for ResourceBound {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let traits = Punctuated::parse_separated_nonempty(input)?;
        Ok(Self { name, traits })
    }
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: Ident,
    pub ty: Type,
}

impl Parse for Parameter {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        Ok(Self {
            name,
            ty: input.parse()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Spawn {
    pub name: Ident,
    /// `Some([])` is an explicit zero-input signature, `report()`. `None` is the
    /// bare name `report`, which carries no signature to bound the closure with.
    pub inputs: Option<Vec<Parameter>>,
}

impl Parse for Spawn {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        let inputs = if input.peek(syn::token::Paren) {
            let body;
            parenthesized!(body in input);
            Some(
                Punctuated::<Parameter, Token![,]>::parse_terminated(&body)?
                    .into_iter()
                    .collect(),
            )
        } else {
            None
        };
        Ok(Self { name, inputs })
    }
}

/// Registry requirements are retained only for the existing prototype adapter.
#[derive(Clone, Debug)]
pub struct TaskDependency {
    pub id: Ident,
    pub features: Vec<LitStr>,
}

impl Parse for TaskDependency {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let id = input.parse()?;
        let mut features = Vec::new();
        if input.peek(syn::token::Paren) {
            let body;
            parenthesized!(body in input);
            let key: Ident = body.parse()?;
            if key != "features" {
                return Err(Error::new(key.span(), "expected `features`"));
            }
            body.parse::<Token![=]>()?;
            features = list(&body)?;
            if !body.is_empty() {
                return Err(body.error("unexpected dependency requirement argument"));
            }
        }
        Ok(Self { id, features })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TaskArguments {
    pub bounds: Vec<ResourceBound>,
    pub local: Vec<Resource>,
    pub shared: Vec<Resource>,
    pub config: Vec<Resource>,
    pub spawn: Vec<Spawn>,
    /// Source reference, not the system's actual tick rate or hardware clock.
    pub monotonic: Option<Path>,
    pub dependencies: Vec<TaskDependency>,
}

fn list<T: Parse>(input: ParseStream<'_>) -> syn::Result<Vec<T>> {
    let body;
    bracketed!(body in input);
    Ok(Punctuated::<T, Token![,]>::parse_terminated(&body)?
        .into_iter()
        .collect())
}

impl Parse for TaskArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self::default();
        let mut keys = BTreeSet::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if !keys.insert(key.to_string()) {
                return Err(Error::new(
                    key.span(),
                    format!("duplicate task argument `{key}`"),
                ));
            }
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "bounds" => args.bounds = list(input)?,
                "local" => args.local = list(input)?,
                "shared" => args.shared = list(input)?,
                "config" => args.config = list(input)?,
                "spawn" => args.spawn = list(input)?,
                "monotonic" => args.monotonic = Some(input.parse()?),
                "dependencies" => args.dependencies = list(input)?,
                _ => {
                    return Err(Error::new(
                        key.span(),
                        "unknown task argument; scheduling belongs in composition",
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(args)
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

impl TaskArguments {
    /// Structural validation of the declaration itself, before any expansion
    /// reads it: duplicate names, empty categories, missing types.
    pub fn validate(&self) -> syn::Result<()> {
        unique(
            self.local.iter().map(|r| &r.name),
            "duplicate local resource",
        )?;
        unique(
            self.shared.iter().map(|r| &r.name),
            "duplicate shared resource",
        )?;
        unique(
            self.config.iter().map(|r| &r.name),
            "duplicate task configuration",
        )?;
        unique(
            self.bounds.iter().map(|r| &r.name),
            "duplicate resource bound",
        )?;
        unique(self.spawn.iter().map(|s| &s.name), "duplicate spawn alias")?;
        unique(
            self.dependencies.iter().map(|d| &d.id),
            "duplicate task dependency",
        )?;
        for resource in &self.shared {
            if let Some(init) = &resource.init {
                return Err(Error::new(
                    init.span(),
                    "only a local resource can have an initial value; a shared \
                     resource is initialized by the firmware's `init`",
                ));
            }
        }
        for config in &self.config {
            if let Some(init) = &config.init {
                return Err(Error::new(
                    init.span(),
                    "a configuration value is supplied by the firmware that \
                     selects the task, not by the definition",
                ));
            }
        }
        for local in &self.local {
            if local.init.is_some() && local.ty.is_none() {
                return Err(Error::new(
                    local.name.span(),
                    "a local resource with an initial value needs its type, \
                     `name: Type = value`, as in RTIC",
                ));
            }
        }
        for local in &self.local {
            if self
                .shared
                .iter()
                .any(|r| identifier_key(&r.name) == identifier_key(&local.name))
            {
                return Err(Error::new(
                    local.name.span(),
                    "a task resource cannot be both local and shared",
                ));
            }
        }
        for dependency in &self.dependencies {
            let mut seen = BTreeSet::new();
            for feature in &dependency.features {
                if !seen.insert(feature.value()) {
                    return Err(Error::new(
                        feature.span(),
                        "duplicate task dependency feature",
                    ));
                }
            }
        }
        for spawn in &self.spawn {
            if let Some(inputs) = &spawn.inputs {
                unique(inputs.iter().map(|p| &p.name), "duplicate spawn parameter")?;
            }
        }
        Ok(())
    }

    /// Check the independently authored SW contract. Rust still checks types.
    pub fn validate_standalone(&self) -> syn::Result<()> {
        self.validate()?;
        if let Some(dependency) = self.dependencies.first() {
            return Err(Error::new(
                dependency.id.span(),
                "standalone task dependencies belong in Cargo.toml",
            ));
        }
        for bound in &self.bounds {
            if !self
                .local
                .iter()
                .chain(&self.shared)
                .any(|r| identifier_key(&r.name) == identifier_key(&bound.name))
            {
                return Err(Error::new(
                    bound.name.span(),
                    "bounded resource must be claimed as local or shared",
                ));
            }
            if bound
                .traits
                .iter()
                .any(|b| !matches!(b, TypeParamBound::Trait(_)))
            {
                return Err(Error::new(
                    bound.name.span(),
                    "resource bounds must be trait bounds",
                ));
            }
        }
        for resource in self.local.iter().chain(&self.shared) {
            let bounded = self
                .bounds
                .iter()
                .any(|b| identifier_key(&b.name) == identifier_key(&resource.name));
            if bounded == resource.ty.is_some() {
                return Err(Error::new(
                    resource.name.span(),
                    "resource needs either an inline type or a resource-keyed bound, not both",
                ));
            }
            if let Some(ty) = &resource.ty {
                explicit_type(ty)?;
            }
        }
        for config in &self.config {
            if config.ty.is_none() {
                return Err(Error::new(
                    config.name.span(),
                    "standalone configuration needs an inline type",
                ));
            }
            explicit_type(config.ty.as_ref().expect("configuration type was checked"))?;
        }
        for spawn in &self.spawn {
            if spawn.inputs.is_none() {
                return Err(Error::new(
                    spawn.name.span(),
                    "standalone spawn alias needs a signature, including `()` for no inputs",
                ));
            }
        }
        Ok(())
    }
}

fn explicit_type(ty: &Type) -> syn::Result<()> {
    if matches!(ty, Type::ImplTrait(_) | Type::Infer(_)) {
        return Err(Error::new(
            ty.span(),
            "declare an explicit type; resource trait requirements belong in `bounds`",
        ));
    }
    Ok(())
}

/// Which RTIC task shape a definition was authored as. RTIC itself makes the
/// same distinction by signature, so the authored `fn` versus `async fn` is
/// what decides it here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// `async fn` - dispatched in software, may take inputs and diverge.
    Software,
    /// `fn` - bound to an interrupt by composition, takes only its context.
    Hardware,
}

#[derive(Clone, Debug)]
pub struct TaskContract {
    pub arguments: TaskArguments,
    pub kind: TaskKind,
    pub context: Ident,
    pub inputs: Vec<Parameter>,
    pub diverges: bool,
}

impl TaskContract {
    pub fn new(arguments: TaskArguments, signature: &Signature) -> syn::Result<Self> {
        arguments.validate_standalone()?;
        if signature.unsafety.is_some()
            || signature.abi.is_some()
            || signature.constness.is_some()
            || !signature.generics.params.is_empty()
            || signature.generics.where_clause.is_some()
            || signature.variadic.is_some()
        {
            return Err(Error::new(
                signature.span(),
                "standalone tasks must be safe functions without generics or an ABI",
            ));
        }
        let kind = if signature.asyncness.is_some() {
            TaskKind::Software
        } else {
            TaskKind::Hardware
        };
        let mut parameters = signature.inputs.iter();
        let context = parameter(
            parameters
                .next()
                .ok_or_else(|| Error::new(signature.span(), "task needs a context parameter"))?,
        )?;
        let Type::Path(context_type) = &context.ty else {
            return Err(Error::new(context.ty.span(), "expected task_name::Context"));
        };
        let segments = &context_type.path.segments;
        if context_type.qself.is_some()
            || context_type.path.leading_colon.is_some()
            || segments.len() != 2
            || identifier_key(&segments[0].ident) != identifier_key(&signature.ident)
            || segments[1].ident != "Context"
            || segments.iter().any(|s| !s.arguments.is_empty())
        {
            return Err(Error::new(context.ty.span(), "expected task_name::Context"));
        }
        let inputs = parameters.map(parameter).collect::<syn::Result<Vec<_>>>()?;
        unique(
            std::iter::once(&context.name).chain(inputs.iter().map(|p| &p.name)),
            "duplicate task parameter",
        )?;
        let diverges = match &signature.output {
            ReturnType::Default => false,
            ReturnType::Type(_, ty) => match ty.as_ref() {
                Type::Never(_) => true,
                Type::Tuple(tuple) if tuple.elems.is_empty() => false,
                _ => {
                    return Err(Error::new(
                        ty.span(),
                        "initial SW tasks must return `()` or `!`",
                    ));
                }
            },
        };
        if kind == TaskKind::Hardware {
            // An interrupt handler is entered by the hardware, so there is no
            // caller to supply inputs and nowhere for a `!` return to go.
            if let Some(extra) = inputs.first() {
                return Err(Error::new(
                    extra.name.span(),
                    "hardware tasks take only their context; an interrupt has no caller to pass inputs",
                ));
            }
            if diverges {
                return Err(Error::new(
                    signature.span(),
                    "hardware tasks must return `()`; an interrupt handler has to return",
                ));
            }
        }
        Ok(Self {
            arguments,
            kind,
            context: context.name,
            inputs,
            diverges,
        })
    }
}

fn parameter(argument: &FnArg) -> syn::Result<Parameter> {
    let FnArg::Typed(argument) = argument else {
        return Err(Error::new(
            argument.span(),
            "task parameters cannot be receivers",
        ));
    };
    let Pat::Ident(pattern) = argument.pat.as_ref() else {
        return Err(Error::new(
            argument.pat.span(),
            "initial task parameters must be named identifiers",
        ));
    };
    if pattern.by_ref.is_some() || pattern.subpat.is_some() {
        return Err(Error::new(
            pattern.span(),
            "initial task parameters cannot use `ref` or subpatterns",
        ));
    }
    Ok(Parameter {
        name: pattern.ident.clone(),
        ty: (*argument.ty).clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn contract(args: &str, function: &str) -> syn::Result<TaskContract> {
        let args = syn::parse_str(args)?;
        let function: syn::ItemFn = syn::parse_str(function)?;
        TaskContract::new(args, &function.sig)
    }
    #[test]
    fn parses_complete_standalone_contract_without_system_types() {
        let task = contract(
            "bounds = [led: StatefulOutputPin + Send], local = [led, count: u32],
             shared = [enabled: bool], config = [period_ms: u32],
             spawn = [report(value: u32), wake()], monotonic = Mono,",
            "pub async fn blink(mut cx: blink::Context, value: (u32, bool)) -> ! { loop {} }",
        )
        .unwrap();
        assert_eq!(task.context, "cx");
        assert_eq!(task.inputs.len(), 1);
        assert_eq!(
            task.inputs[0].ty.to_token_stream().to_string(),
            "(u32 , bool)"
        );
        assert!(task.diverges);
        assert_eq!(task.arguments.bounds[0].traits.len(), 2);
        assert_eq!(task.arguments.local[1].name, "count");
        assert_eq!(task.arguments.shared[0].name, "enabled");
        assert_eq!(task.arguments.config[0].name, "period_ms");
        assert_eq!(task.arguments.spawn[1].inputs.as_ref().unwrap().len(), 0);
        assert!(task.arguments.monotonic.unwrap().is_ident("Mono"));
    }

    #[test]
    fn distinguishes_an_explicit_zero_input_signature_from_a_bare_name() {
        let args: TaskArguments = syn::parse_str("spawn = [wake(), report]").unwrap();
        assert!(args.spawn[0].inputs.as_ref().unwrap().is_empty());
        assert!(args.spawn[1].inputs.is_none());
        assert!(
            args.validate_standalone()
                .unwrap_err()
                .to_string()
                .contains("signature")
        );
    }

    #[test]
    fn rejects_invalid_resource_and_configuration_contracts() {
        for (args, message) in [
            ("bounds = [led: Pin]", "must be claimed"),
            ("local = [led]", "either an inline type"),
            ("bounds = [led: Pin], local = [led: u32]", "not both"),
            (
                "local = [state: bool], shared = [state: bool]",
                "both local and shared",
            ),
            (
                "shared = [state: bool, state: bool]",
                "duplicate shared resource",
            ),
            ("config = [period_ms]", "inline type"),
            ("local = [led: impl Pin]", "explicit type"),
            ("config = [period_ms: _]", "explicit type"),
            ("config = [n: u32, n: u32]", "duplicate task configuration"),
            (
                "spawn = [report(x: u32, x: bool)]",
                "duplicate spawn parameter",
            ),
            ("dependencies = [Fugit]", "Cargo.toml"),
            ("bounds = [led: 'static], local = [led]", "trait bounds"),
            ("shared = [state: bool = false]", "only a local resource"),
            ("config = [period_ms: u32 = 5]", "supplied by the firmware"),
            ("local = [count = 0]", "needs its type"),
            ("bounds = [led: Pin], local = [led: u32 = 0]", "not both"),
        ] {
            let error = contract(args, "async fn run(cx: run::Context) {}").unwrap_err();
            assert!(error.to_string().contains(message), "{args}: {error}");
        }
    }

    #[test]
    fn rejects_duplicate_keys_missing_commas_and_dependency_trailing_tokens() {
        for source in [
            "local = [a], local = [b]",
            "local = [a] shared = [b]",
            "dependencies = [D(features = [], unexpected)]",
            "priority = 1",
        ] {
            assert!(syn::parse_str::<TaskArguments>(source).is_err(), "{source}");
        }
    }

    #[test]
    fn accepts_synchronous_hardware_handlers() {
        let task = contract("", "fn on_tick(cx: on_tick::Context) {}").unwrap();
        assert_eq!(task.kind, TaskKind::Hardware);
        assert!(task.inputs.is_empty());
        assert!(!task.diverges);

        let task = contract("", "async fn run(cx: run::Context) {}").unwrap();
        assert_eq!(task.kind, TaskKind::Software);
    }

    #[test]
    fn rejects_hardware_handlers_that_cannot_be_entered_by_an_interrupt() {
        for source in [
            // an interrupt has no caller to supply inputs
            "fn on_tick(cx: on_tick::Context, value: u32) {}",
            // an interrupt handler has to return
            "fn on_tick(cx: on_tick::Context) -> ! { loop {} }",
        ] {
            assert!(contract("", source).is_err(), "{source}");
        }
    }

    #[test]
    fn rejects_unsupported_task_signatures() {
        for source in [
            "async fn run() {}",
            "async fn run(cx: other::Context) {}",
            "async fn run(cx: run::Context<'static>) {}",
            "async fn run<T>(cx: run::Context) {}",
            "async fn run(cx: run::Context) -> u32 { 0 }",
            "async fn run(cx: run::Context, (a,b): (u32,u32)) {}",
            "async fn run(cx: run::Context, cx: u32) {}",
        ] {
            assert!(contract("", source).is_err(), "{source}");
        }
    }

    #[test]
    fn accepts_returning_tasks_and_raw_parameter_names() {
        let task = contract("", "async fn run(ctx: run::Context, r#type: u32) -> () {}").unwrap();
        assert!(!task.diverges);
        assert_eq!(task.context, "ctx");
        assert_eq!(task.inputs.len(), 1);
    }
}
