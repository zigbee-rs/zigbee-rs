# ESP Zigbee sensor

ESP sensor application to work with the [zigbee](https://crates.io/crates/zigbee) crate.

Supports all Espressif SoCs that have an 802.15.4 radio, selected by a cargo feature.
All three use the `riscv32imac-unknown-none-elf` target, so nothing else in the setup changes.

| Chip | Feature | Build |
|------|---------|-------|
| ESP32-C6 | `esp32c6` | `cargo run --release --features esp32c6` |
| ESP32-H2 | `esp32h2` | `cargo run --release --features esp32h2` |
| ESP32-C5 | `esp32c5` | `cargo run --release --features esp32c5` |

## Usage
Download and install the [espflash](https://github.com/esp-rs/espflash/releases) tool, a serial flasher utility for [Espressif](https://www.espressif.com/) SoCs.

Delete previous firmware images from the Hardware to avoid side-effects.
```sh
espflash erase-flash
```

Flash the pre-built application to the Hardware.
```sh
espflash flash firmware.bin
```


## Build
Follow the [ESP Book](https://docs.esp-rs.org/book/installation/index.html) for prerequisites.

Install the target and run the application to build and flash the image onto an ESP device.
`espflash` detects the connected chip, so the runner needs no `--chip` argument.
```sh
cargo run --release --features esp32c6   # or esp32h2 / esp32c5
# opt: update the pre-built application in the repository
cp target/riscv32imac-unknown-none-elf/release/esp-sensor firmware.bin
```

## Use it in your own project

The manifest here depends on the crates by path so the example builds against
this repository. To start from it, drop the paths and take the published crates:

```toml
[dependencies]
zigbee = { version = "0.1.0-alpha.5", features = ["storage"] }
zigbee-types = "0.1.0-alpha.5"
zigbee-mac = { version = "0.1.0-alpha.5", features = ["esp32c6"] }
zigbee-cluster-library = "0.1.0-alpha.5"
```
