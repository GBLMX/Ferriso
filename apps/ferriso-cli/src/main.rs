//! The `ferriso` command-line entry point.
//!
//! Clawless owns argument parsing, command discovery, async execution, and
//! output presentation. This crate only contributes the framework catalog and
//! the commands that expose it.

mod catalog;
mod commands;

clawless::main!();
