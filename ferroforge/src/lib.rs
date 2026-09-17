#![no_std]

//! FerroForge composes reusable RTIC tasks and project-owned initialization
//! into a real RTIC firmware application.
//!
//! The two macros are named as RTIC names the same ideas: `#[ferroforge::task]`
//! marks a task definition, `ferroforge::app!` declares the application that
//! selects it.
//!
//! This crate is the facade: it re-exports them and contributes nothing at
//! runtime, which is why a task crate marks it check-only.

pub use ferroforge_macros::{app, task};
