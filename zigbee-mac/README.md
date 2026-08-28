# Zigbee MAC Interface

This crate contains the required MAC layer primitives of the IEEE 802.15.4 standard used by the Zigbee's protocol higher layers.

The `Mlme` trait is the seam between the stack and the radio: everything above it
is generic over the implementation, so the stack itself stays free of any chip or
vendor dependency.

## Contents

| Area | Items |
|------|-------|
| MLME | `Mlme` — scan, associate, transmit and receive primitives driven by the NWK layer |
| Configuration | `MacConfig`, `ScanType`, `ScanResult`, `PanDescriptor`, `AssociationResponse` |
| Addressing | `Address`, `ExtendedAddress`, `PanId`, `MacShortAddress`, re-exported from [`ieee802154`](https://crates.io/crates/ieee802154) |
| Errors | `MacError` |

## Radio backends

| Backend | Feature | Status |
|---------|---------|--------|
| ESP32-C6 | `esp32c6` | available |
| any other radio | — | implement `Mlme` yourself |

No backend is enabled by default. Implementing `Mlme` is the only thing a new
platform needs; nothing above this crate has to change.

## 🏛️ License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## 🧩 Contribution

This is a free and open project and lives from contributions of the community.

See our [Contribution Guide](CONTRIBUTING.md)
