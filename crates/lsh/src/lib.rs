// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//!
//! Vendored LSH runtime and generated language definitions.
//!
//! The interpreter is copied from `microsoft/edit` (MIT). The definitions were
//! generated from that repository's `crates/lsh/definitions` directory with
//! `lsh::compiler::Generator::generate_rust`; the compiler itself is not part
//! of this crate.

pub mod definitions;
pub mod runtime;
