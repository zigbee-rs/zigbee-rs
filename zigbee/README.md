# zigbee

The crate an application depends on: it re-exports the layers of the stack —
`nwk`, `aps`, `security` and `storage` from [`zigbee-core`](https://crates.io/crates/zigbee-core),
the cluster library as `zcl`, the commissioning procedures as `bdb`, the MAC
abstraction as `mac` and the shared types as `types` — and adds the runtime
that drives them.

The layers below implement the specification and nothing else. This crate holds
the behavior the specs deliberately leave to the implementer, so an application
does not have to reinvent it:

| Function | What it decides |
|----------|-----------------|
| `commission` | startup order: resume, rejoin, or steer onto a network |
| `rx_loop` | receive/dispatch strategy and poll cadence |
| `link_maintenance` | keepalive period (3.6.10.3) and how to recover a lost parent link |
| `keepalive` | one keepalive, when the loop is driven elsewhere |

`rx_loop` and `link_maintenance` are meant to run as two tasks sharing a
`&'static ZigbeeDevice`.

## Logging

The stack logs through [`defmt-or-log`](https://crates.io/crates/defmt-or-log),
so the backend is a feature of this crate. `log` is the default; `defmt`
replaces it, and the application picks one:

```toml
[dependencies]
zigbee = { version = "0.1", default-features = false, features = ["defmt"] }
```

Without `default-features = false` both backends are on and every record is
emitted twice — once to each. The feature reaches every crate below, so a
single switch moves the whole stack, and the public types carry
`defmt::Format` alongside their `Debug` wherever `defmt` is enabled.

A `defmt` build needs a transport in the application (`defmt-rtt`,
`esp-println` with its `defmt-espflash` feature, …) and `defmt`'s host tooling
to decode what arrives; a `log` build prints wherever the chosen logger points.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
