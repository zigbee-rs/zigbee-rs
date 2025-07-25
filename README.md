# Zigbee in Rust 

![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/thebino/zigbee-rs/ci.yaml?style=for-the-badge)
[![GitHub contributors](https://img.shields.io/github/contributors/thebino/zigbee-rs?color=success&style=for-the-badge)](https://github.com/thebino/zigbee-rs/graphs/contributors)
[![License](https://img.shields.io/github/license/thebino/zigbee-rs?style=for-the-badge)](./LICENSE.md)
[![Matrix](https://img.shields.io/matrix/zigbee:matrix.org?style=for-the-badge)](https://matrix.to/#/#zigbee:matrix.org)

_Zigbee is a wireless communication technology designed for low-power devices in smart homes and industrial settings._

_It enables these devices to connect and communicate with each other, allowing for efficient control and automation of various systems._

> ⚠️⚠️⚠️
> 
> This repository is not yet functional. We appreciate your patience and welcome any [contribution](CONTRIBUTING.md)
>
> We're actively working on adding more features and aim to fully implement the specification over time.



---

In this repository:
- [ZigBee Stack](./zigbee/README.md)

  The core network layer and security features.
  Deals with addressing, keys, trust center, formation and discovery mechanisms.
  For more, see the official [ZigBee Specification Rev. 23](https://csa-iot.org/wp-content/uploads/2024/07/docs-05-3474-23-csg-zigbee-specificationR23.1.pdf)

- [ZigBee Base Device Behavior](./zigbee-base-device-behavior/README.md)

  Defines the standard commissioning procedures all devices must support.
  Provides a high-level abstraction over the zigbee stack.
  For more, see the official [ZigBee Base Device Behavior Specification Rev. 13](https://csa-iot.org/wp-content/uploads/2022/12/16-02828-012-PRO-BDB-v3.0.1-Specification.pdf)

- [ZigBee Cluster Library](./zigbee-cluster-library/README.md)

  Defines application-level behaviors, like reading attributes, reporting, and commands.
  Contains standard clusters like Temperature Measurement, Basic Identify, etc.
  For more, see the official [ZigBee Cluster Library Rev 8](https://csa-iot.org/wp-content/uploads/2022/01/07-5123-08-Zigbee-Cluster-Library-1.pdf)

---

## 🏛️ License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## 🧩 Contribution

This is a free and open project and lives from contributions of the community.

See our [Contribution Guide](CONTRIBUTING.md)

