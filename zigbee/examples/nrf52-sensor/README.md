# nRF52 ZigBee sensor

nRF sensor application to work with the [zigbee](https://crates.io/crates/zigbee) crate. 

## Usage
Download and install the [nRF Util](https://www.nordicsemi.com/Products/Development-tools/nrf-util), an unified command line utility for Nordic products. 

Install additional commans:
```sh
nrfutil install completion device nrf5sdk-tools jlink
```

Delete previous firmware images from the Hardware to avoid side-effects.
```shell
nrfutil device erase --traits nordicDFU  # for nRF52840-Dongle
nrfutil device erase --traits jlink      # for nRF52833 & nRF52840
```

Flash the pre-built application to the Hardware.
```sh
nrfutil device program --firmware zephyr.zip --traits nordicDFU  # for nRF52840-Dongle
nrfutil device program --firmware zephyr.zip --traits jlink      # for nRF52833 & nRF52840
```


## Build

Install the target and run the application to build the application.
```sh
rustup target add thumbv7em-none-eabihf
cargo build
```

Convert the ELF to a hex blob
```shell
cargo objcopy -- -O ihex zephyr.hex
```

Convert the hex blob into a running firmware zip
```sh
nrfutil pkg generate --hw-version 52 --sd-req=0x00 --application zephyr.hex --application-version 1 zephyr.zip

```

Flash the application to the Hardware
```sh
nrfutil device program --firmware zephyr.zip --traits nordicDFU  # for nRF52840-Dongle
nrfutil device program --firmware zephyr.zip --traits jlink      # for nRF52833 & nRF52840
```

