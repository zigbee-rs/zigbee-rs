# Zigbee Types

Shared `no-std` types for the [`zigbee`](https://crates.io/crates/zigbee) protocol stack.

The primitives every layer of the stack agrees on: 
 * addresses
 * wire-format helpers
 * fixed-capacity collections
 * async building blocks used to await radio events without an allocator

## Contents

| Area | Items |
|------|-------|
| Addresses | `ShortAddress`, `IeeeAddress`, `NwkAddress` |
| Wire format | `ByteArray`, `ByteArrayRef`, `TypeArrayRef` — fixed and borrowed byte/element slices that serialize through the [`byte`](https://crates.io/crates/byte) crate |
| Collections | `StorageVec<T, N>` — fixed-capacity vector over [`heapless`](https://crates.io/crates/heapless), with an `alloc` variant behind a feature |
| Descriptor fields | `MacCapabilityFlagsField`, `ServerMaskField` — bitfields from the Zigbee node descriptor |
| `sync` | `Signal<T>`, `Event`, `with_timeout`, `yield_now` — single-waiter async primitives for a `no-std` executor |
| `storage` | `InMemoryStorage` — an `embedded-storage` backend for tests and hosts without flash |

## 🏛️ License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## 🧩 Contribution

This is a free and open project and lives from contributions of the community.

See our [Contribution Guide](CONTRIBUTING.md)
