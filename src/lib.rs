//! Terragrunt TUI: local, read-only Terragrunt plan review.

mod cli;
mod diff;
mod engine;
mod graph;
mod model;
mod theme;
mod ui;
mod update;

pub use cli::main;
