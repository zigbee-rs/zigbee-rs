# esp-discovery

Scans Zigbee channels 11–26 for nearby networks and prints the results to the serial monitor.
After scanning it displays each unique PAN, its extended PAN ID, channel, and permit-join status, followed by the full neighbor table built from the scan.

## Configuration

Edit the constants at the top of `src/main.rs` before building:

| Constant | Default | Description |
|---|---|---|
| `CHANNELS` | `11..27` | Channel range to scan |
| `SCAN_DURATION` | `10` | Scan duration exponent (longer = more time per channel) |

## Prerequisites

Follow the [ESP-RS book](https://docs.esp-rs.org/book/installation/index.html) to install the Rust toolchain and target. Then install the flasher:

```sh
cargo install espflash
```

## Build & Flash

```sh
cargo run --release
```

`cargo run` compiles the binary and immediately flashes it via `espflash flash --monitor`, opening the serial monitor automatically.

To build without flashing:

```sh
cargo build --release
```

The binary is written to `target/riscv32imac-unknown-none-elf/release/esp-discovery`.

## Expected output

```
Starting network discovery, channels: 11..27, duration: 10
Found following Zigbee networks in proximity:
[
    NetworkDescriptor {
        pan_id: ShortAddress(0x1a2b),
        ...
    },
]
Neighbor Table:
[...]
```
