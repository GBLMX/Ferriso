# lsh

Vendored bytecode runtime for syntax highlighting, derived from
[`microsoft/edit`](https://github.com/microsoft/edit) via the upstream `main`
tarball retrieved on 2026-10-22. No git clone is required.

- Runtime: `crates/lsh/src/runtime.rs`, copied from the upstream
  repository's MIT-licensed source with two compatibility edits replacing
  APIs stabilized after Rust 1.88 (`write_copy_of_slice` and
  `MaybeUninit::assume_init_ref`).
- Definitions: `crates/lsh/src/definitions.rs`, generated from
  `crates/lsh/definitions/*.lsh` with `lsh::compiler::Generator::generate_rust`.
  The build-time compiler is intentionally not included.
- License: MIT; see `LICENSE` and the copyright headers in the source files.
