# Zigbee Macros

Declarative macros backing the [`zigbee`](https://crates.io/crates/zigbee) protocol stack.

> **Internal API.** Both macros are `#[doc(hidden)]` and exist to serve the other `zigbee-*` crates. 
> They are published because macro expansion happens in the consumer, not because they are meant to be used directly 
> expect breaking changes without a major version bump.

## Contents

| Macro | Purpose |
|-------|---------|
| `impl_byte!` | Derives `TryRead`/`TryWrite` from the [`byte`](https://crates.io/crates/byte) crate for structs, newtypes and tag-dispatched enums, so a frame layout is declared once and serializes itself |
| `construct_ib!` | Builds an information base (the NWK NIB, the APS AIB): a singleton with per-field locked accessors, `update_<field>` mutators, dirty tracking and optional flash-backed persistence |

## Toolchain

Requires a nightly toolchain: `construct_ib!` uses the unstable
[`macro_metavar_expr_concat`](https://github.com/rust-lang/rust/issues/124225)
feature to derive identifiers from the information base name.

## 🏛️ License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## 🧩 Contribution

This is a free and open project and lives from contributions of the community.

See our [Contribution Guide](CONTRIBUTING.md)
