# zigbee-app

Application-facing runtime for the [`zigbee`](../zigbee) stack.

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

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
