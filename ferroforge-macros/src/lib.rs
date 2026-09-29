//! FerroForge's procedural macros.
//!
//! Three entry points, named as RTIC names the same ideas and, where RTIC has
//! no word, as AADL does. `#[ferroforge::task]` turns a task definition into an
//! ordinary generic function and a real context type; `#[ferroforge::group]`
//! marks a module of definitions as a task group, selected together;
//! `ferroforge::app!` expands a firmware in place into a real `#[rtic::app]`
//! whose handlers construct that context and call the function.
//!
//! None reads another crate's source. `app!` emits real Rust paths into the
//! task crates a firmware depends on, so a wrong definition, binding or type is
//! an ordinary compile error at the authored line; a task group's members
//! reach it as tokens its own macro hands back.

#![deny(missing_docs)]

mod app;
mod group;
mod task;

use ferroforge_contracts::{TaskArguments, TaskContract};
use proc_macro::TokenStream;
use syn::{ItemFn, parse_macro_input};

/// The firmware's authored application, expanded in place into a real
/// `#[rtic::app]`. Init and resources are written here and never move; each
/// task declaration becomes an adapter that calls the selected definition.
#[proc_macro]
pub fn app(input: TokenStream) -> TokenStream {
    let application = parse_macro_input!(input as app::App);
    match app::expand(application) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// A task group: a module of task definitions that only work together, named
/// after AADL's thread group and selected by a firmware in one
/// `#[group(from = ..)] mod name { .. }` inside `app!`. The
/// module is left as written; alongside it goes an exported `macro_rules!`
/// that hands its members to `app!`.
#[proc_macro_attribute]
pub fn group(attr: TokenStream, item: TokenStream) -> TokenStream {
    let arguments = parse_macro_input!(attr as group::GroupArguments);
    let module = parse_macro_input!(item as syn::ItemMod);
    match group::define(arguments, module) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// A task definition: expands to a real generic context and an ordinary generic
/// function, with no mock layer. A firmware depends on this crate normally and
/// its RTIC handler calls the function, so no source is transplanted and the
/// body is compiled once, in place.
#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as TaskArguments);
    let function = parse_macro_input!(item as ItemFn);

    let (inside, beside) = group::solo(&args, &function);
    let output = TaskContract::new(args, &function.sig)
        .and_then(|contract| task::expand(contract, function, inside))
        .map(|output| quote::quote!(#output #beside));

    match output {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
