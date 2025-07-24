# Zigbee Stack

A `no_std` ZigBee Protocol Stack implementation based on the [ZigBee specification 23](https://csa-iot.org/wp-content/uploads/2024/07/docs-05-3474-23-csg-zigbee-specificationR23.1.pdf)

The core network layer and security features. Deals with addressing, keys, trust center, formation and discovery mechanisms.

---

```mermaid
sequenceDiagram
    participant Device
    participant Coordinator
    rect rgb(255, 120, 120)
    note right of Device: Unencrypted
    Device->>Coordinator: Beacon Request (0x07)
    Coordinator-->>Device: Zigbee Beacon
    Device->>Coordinator: Association Request (0x01)
    Coordinator-->>Device: Association Request
    end
    rect rgb(255, 180, 100)
    note right of Device: APS encrypted only
    Coordinator-->>Device: Transport Key
    end
    rect rgb(125, 235, 150)
    note right of Device: NWK encrypted
    Device->>Coordinator: Device Announcement
    end
```

## 🏛️ License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## 🧩 Contribution

This is a free and open project and lives from contributions of the community.

See our [Contribution Guide](CONTRIBUTING.md)

