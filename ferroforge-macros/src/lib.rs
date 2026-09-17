//! FerroForge's procedural macros.
//!
//! Two entry points, and nothing between them, named as RTIC names the same
//! ideas. `#[ferroforge::task]` turns a task definition into an ordinary
//! generic function and a real context type; `ferroforge::app!` expands a
//! firmware in place into a real `#[rtic::app]` whose handlers construct that
//! context and call the function.
//!
//! Neither reads the other's crate. `app!` emits real Rust paths into the task
//! crates a firmware depends on, so a wrong definition, binding or type is an
//! ordinary compile error at the authored line.

mod app;
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

/// A task definition: expands to a real generic context and an ordinary generic
/// function, with no mock layer. A firmware depends on this crate normally and
/// its RTIC handler calls the function, so no source is transplanted and the
/// body is compiled once, in place.
#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as TaskArguments);
    let function = parse_macro_input!(item as ItemFn);

    let output = TaskContract::new(args, &function.sig)
        .and_then(|contract| task::expand(contract, function));

    match output {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
