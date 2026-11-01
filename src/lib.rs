//! Terragrunt TUI: local, read-only Terragrunt plan review.

mod cli;
mod engine;
mod graph;
mod model;
mod ui;

pub use cli::main;
