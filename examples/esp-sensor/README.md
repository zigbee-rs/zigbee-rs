# ESP ZigBee sensor

ESP sensor application to work with the [zigbee](https://crates.io/crates/zigbee) crate.

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
```sh
rustup target add xtensa-esp32c6-espidf
cargo run --release
```

