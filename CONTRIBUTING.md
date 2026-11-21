# Welcome to Zigbee in Rust

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

## 🤖 AI Contribution Policy

AI tools (LLMs, coding agents, autocomplete) are allowed in this project. However, **you are the author** of everything you submit, and you are fully responsible for it, no matter how it was produced.

### ✅ You are accountable

 * **Understand every line.** You must be able to explain and defend every change in review. "The AI wrote it" is not an answer to a review question.
 * **Review your own work first.** Read your full diff before asking anyone else to.
 * **Keep it small and focused.** One logical change per PR. Large, unfocused or obviously unreviewed generated PRs may be closed without a detailed review.
 * **Licensing.** You make sure that submitted content does not reproduce code under an incompatible license and can be released under the project's MIT / Apache-2.0 licenses.

### 📖 Follow the specification

LLMs are known to invent plausible-looking but wrong protocol details, such as frame fields, attribute ids, status codes or state transitions. For every change to protocol logic (MAC, NWK, APS, ZDO, ZCL, BDB):

 * **Reference the specification section** you implemented (Zigbee R22 or IEEE 802.15.4-2003) in the PR description and, where useful, in code comments
 * **Don't add behavior the specification doesn't define.** Implementation-specific policy belongs in the `zigbee` app runtime crate

### 🔬 Test on real hardware

CI runs builds, tests, Clippy and formatting checks. What it can't do is show that the stack works on a real network. For new features and larger changes to protocol logic, **test on real hardware** and describe the setup in the PR:

 * **Device:** board and chip (e.g. ESP32-C6) and the example or firmware you ran
 * **Network:** coordinator and software (e.g. Sonoff ZBDongle-E with Zigbee2MQTT, nRF52840 DK with the nRF Zigbee Shell) and the channel
 * **Scenario:** what you exercised (e.g. join, rejoin after power cycle, leave, attribute reporting) and the result, with logs or a sniffer capture where useful

Also say what you did **not** test. Never claim hardware testing that didn't happen.

### 🐛 Bug reports and security issues

Don't paste raw AI output into issues or comments. Verify AI-found bugs yourself and include a reproduction (steps, logs or a sniffer capture) before filing. The same applies to security reports.

### 🚫 Not allowed

 * **Autonomous agents.** Bots or agents must not open PRs or issues, or comment, without a human reviewing and submitting the content
 * **Unreviewed AI reviews.** AI-generated review comments must be checked by a human before posting

Maintainers may close contributions that don't follow this policy.

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

💡 When selecting the `nRF Sniffer for 802.15.4` device in wireshark, use Nordics default **Channel 16** for capturing if not configured otherwise.


#### nRF Zigbee Shell

First build and flash [the Zigbee Shell firmware](https://docs.nordicsemi.com/bundle/addon-zigbee-r23-latest/page/samples/shell/README.html) to a Development Kit.

Next install the [nRF Connect for Desktop](https://www.nordicsemi.com/Products/Development-tools/nrf-connect-for-desktop) followed by install and start of the **Serial Terminal**

After selecting the Development Kit in this terminal, run the following commands to use it as **Zigbee Coordinator**

```shell
nvram disable
bdb nwkkey abcdef01234567890000000000000000
bdb channel 16
bdb role zc
nvram enable
bdb start
bdb ic add 83FED3407A939723A5C639B26916D505C3B5 0B010E2F79E9DBFA
```

See [Zigbee shell command list](https://docs.nordicsemi.com/bundle/addon-zigbee-r23-latest/page/lib/shell.html#zigbee_shell_command_list) for more.

🎉🎉🎉 Now it should be possible to form a network and add new devices by joining. 🎉🎉🎉

