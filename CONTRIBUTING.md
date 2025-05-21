# Welcome to ZigBee in Rust

This is a **FOSS** (free and open-source software) and lives from contributions of the community.

There are many ways to contribute:

 * 📣 Spread the project or its apps to the world
 * ✍️ Writing tutorials and blog posts
 * 📝 Create or update the documentation
 * 🐛 Submit bug reports
 * 💡 Adding ideas and feature requests to Discussions
 * 👩‍🎨 Create designs or UX flows
 * 🧑‍💻 Contribute code or review PRs


## 📜 Ground Rules

A community like this should be **open**, **considerate** and **respectful**.

Behaviours that reinforce these values contribute to a positive environment, and include:

 * **Being open**. Members of the community are open to collaboration.
 * **Focusing on what is best for the community**. We're respectful of the processes set forth in the community, and we work within them.
 * **Acknowledging time and effort**. We're respectful and thoughtful when addressing the efforts of others, keeping in mind that often times the labor was completed simply for the good of the community.
 * **Being respectful of differing viewpoints and experiences**. We're receptive to constructive comments and criticism, as the experiences and skill sets of other members contribute to the whole of our efforts.
 * **Showing empathy towards other community members**. We're attentive in our communications, whether in person or online, and we're tactful when approaching differing views.
 * **Being considerate**. Members of the community are considerate of their peers.
 * **Being respectful**. We're respectful of others, their positions, their skills, their commitments, and their efforts.
 * **Gracefully accepting constructive criticism**. When we disagree, we are courteous in raising our issues.
 * **Using welcoming and inclusive language**. We're accepting of all who wish to take part in our activities, fostering an environment where anyone can participate and everyone can make a difference.


## 🧑‍💻 Code Contribution

To contribute code to the repository, you don't need any permissions.
First start by forking the repository, clone and checkout your clone and start coding.
When you're happy with your changes, create Atomic commits on a **new feature branch** and push it to ***your*** fork.

Atomic commits will make it easier to track down regressions. Also, it enables the ability to cherry-pick or revert a change if needed.

1. Fork it (https://github.com/thebino/zigbee-rs/fork)
2. Create a new feature branch (`git checkout -b feature/fooBar`)
3. Commit your changes (`git commit -am 'Add some fooBar'`)
4. Push to the branch (`git push origin feature/fooBar`)
5. Create a new Pull Request

## 💾 Technology

The project is written in [Rust](https://rust-lang.org/) and using `no_std` which is a bare metal approach.

Using `no_std` doesn't use the Rust **standard library** but instead uses a subset, the `core` library.

**The embedded Rust book** has a great [section](https://docs.rust-embedded.org/book/intro/no-std.html) on this.

### 🐛 Debugging

For debugging the communication using this zigbee library, Nordic Semiconductor provides a great tooling for that.

1. [nRF Sniffer](https://www.nordicsemi.com/Products/Development-tools/nRF-Sniffer-for-802154) is able to capture Thread and Zigbee packets with Wireshark
2. [nRF Zigbee Shell](https://docs.nordicsemi.com/bundle/addon-zigbee-r23-latest/page/samples/shell/README.html) is able to mimic a Zigbee router or coordinator.

#### nRF Sniffer

Follow [this guide](https://docs.nordicsemi.com/bundle/ug_sniffer_802154/page/UG/sniffer_802154/installing_sniffer_802154.html) for installing the nRF Sniffer firmware on a development kit or dongle.

Next add the [nRF Sniffer capture plugin](https://docs.nordicsemi.com/bundle/ug_sniffer_802154/page/UG/sniffer_802154/installing_sniffer_802154_plugin.html#installing_sniffer_802154_pluginb) to Wireshark read the captured frames.


#### nRF Zigbee Shell

First build and flash [the Zigbee Shell firmware](https://docs.nordicsemi.com/bundle/addon-zigbee-r23-latest/page/samples/shell/README.html) to a Development Kit.

Next install the [nRF Connect for Desktop](https://www.nordicsemi.com/Products/Development-tools/nrf-connect-for-desktop) followed by install and start of the **Serial Terminal**

After selecting the Development Kit in this terminal, run the following commands to use it as **Zigbee Coordinator**

```shell
nvram disable
bdb nwkkey abcdef01234567890000000000000000
bdb channel 16
nvram enable
bdb role zc
bdb start
```

🎉🎉🎉 Now it should be possible to form a network and add new devices by joining. 🎉🎉🎉

