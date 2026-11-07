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
| `RejoinConfig` | how often a failed rejoin is retried before the network is given up (BDB 7.1 step 5) |
| `rx_loop` | receive/dispatch strategy and poll cadence |
| `link_maintenance` | keepalive period (3.6.10.3) and how to recover a lost parent link |
| `keepalive` | one keepalive, when the loop is driven elsewhere |

`rx_loop` and `link_maintenance` are meant to run as two tasks sharing a
`&'static ZigbeeDevice`.

## Rejoining

A device that was on a network rejoins it at startup, keeping its keys (BDB
7.1). That rejoin fails for reasons that pass by themselves — after a power
cut this device may be up well before the router it rejoins through — so it is
retried on the schedule in `RejoinConfig`: five attempts over about two
minutes by default. Only when the attempts are used up, or the parent denies
access because this device is no longer a member, is the remembered network
forgotten and network steering run, which needs the network opened for joining
again.

Set `attempts: None` for a mains-powered device that should never give its
network up, or `steer_after_failure: false` to be told the rejoin failed
(`Commissioned::Failed`) while the keys are kept, and decide in the
application whether to sleep, reset or re-commission:

```rust,ignore
let config = StackConfig::new(network, device, timing, descriptors)
    .with_rejoin(RejoinConfig {
        attempts: None,
        ..RejoinConfig::default()
    });
```

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
