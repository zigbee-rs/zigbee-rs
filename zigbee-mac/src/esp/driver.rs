use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::efuse;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Error as Ieee802154Error;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;

static TX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static RX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Derive an EUI-64 extended address from a 6-byte EUI-48 MAC.
///
/// Inserts `0xFF, 0xFE` after the OUI (first 3 bytes) per the
/// IEEE EUI-64 conversion convention:
///   `AA:BB:CC:DD:EE:FF` → `AA:BB:CC:FF:FE:DD:EE:FF`
fn eui48_to_eui64(mac: [u8; 6]) -> u64 {
    u64::from_be_bytes([mac[0], mac[1], mac[2], 0xFF, 0xFE, mac[3], mac[4], mac[5]])
}

/// Await the next RX-available signal.
///
/// Operates only on the module-global signal, so it can be awaited **without
/// holding the driver lock** — letting a receive loop idle-wait while a
/// concurrent transmit still acquires the lock. Does not reset beforehand, so a
/// signal raised between a queue drain and this call is not lost.
pub(crate) async fn wait_rx_signal() {
    RX_SIGNAL.wait().await;
}

pub struct Ieee802154Driver<'a> {
    driver: Ieee802154<'a>,
    config: Config,
    /// IEEE 802.15.4 extended (EUI-64) address derived from the
    /// ESP32's factory-burned eFuse MAC address.
    ieee_address: ieee802154::mac::ExtendedAddress,
}

impl<'a> Ieee802154Driver<'a> {
    pub fn new(ieee802154: Ieee802154<'a>, config: Config) -> Self {
        let mac_bytes: [u8; 6] = efuse::base_mac_address()
            .as_bytes()
            .try_into()
            .expect("6-byte MAC");
        let ieee_address = ieee802154::mac::ExtendedAddress(eui48_to_eui64(mac_bytes));

        let mut driver = Self {
            driver: ieee802154,
            config,
            ieee_address,
        };

        driver
            .driver
            .set_rx_available_callback_fn(Self::rx_callback);
        driver.driver.set_tx_done_callback_fn(Self::tx_callback);

        driver.config.rx_when_idle = true;
        driver.config.ext_addr = Some(ieee_address.0);
        driver.update_driver_config(|_| {});

        driver
    }

    /// The device's IEEE 802.15.4 extended (EUI-64) address.
    pub fn ieee_address(&self) -> ieee802154::mac::ExtendedAddress {
        self.ieee_address
    }

    /// The assigned short address, if any.
    pub fn short_address(&self) -> Option<u16> {
        self.config.short_addr
    }

    pub fn update_driver_config(&mut self, update_fn: impl Fn(&mut Config)) {
        update_fn(&mut self.config);
        self.config.rx_when_idle = true;
        self.config.ext_addr = Some(self.ieee_address.0);
        self.driver.set_config(self.config);
    }

    fn rx_callback() {
        RX_SIGNAL.signal(());
    }

    fn tx_callback() {
        TX_SIGNAL.signal(());
    }

    /// Transmit a frame. The radio automatically returns to RX mode
    /// after transmission completes (`rx_when_idle = true`).
    ///
    /// For an acknowledgment-requested frame the transmit-done signal fires
    /// only after the ACK is received or the hardware ACK-wait times out, so
    /// [`Self::last_tx_acked`] is valid once this returns.
    pub async fn transmit(&mut self, frame: &[u8]) -> Result<(), Ieee802154Error> {
        TX_SIGNAL.reset();
        self.driver.transmit_raw(frame, true)?;
        TX_SIGNAL.wait().await;
        Ok(())
    }

    /// Whether the most recent transmission was acknowledged. Only meaningful
    /// for acknowledgment-requested frames.
    pub fn last_tx_acked(&self) -> bool {
        self.driver.get_ack_frame().is_some()
    }

    /// The frame-pending bit of the acknowledgment to the most recent
    /// transmission, if one was received. `Some(true)` means the recipient has
    /// data buffered for this device (IEEE 802.15.4 §7.2.1.1.3).
    pub fn last_ack_frame_pending(&self) -> Option<bool> {
        // data[0] is the PHR length; data[1] is the low byte of the MAC frame
        // control field, whose bit 4 is the frame-pending flag
        self.driver
            .get_ack_frame()
            .and_then(|ack| ack.data.get(1).map(|fcf| fcf & 0x10 != 0))
    }

    /// Poll the hardware RX queue for one frame (non-blocking).
    pub fn poll_received(&mut self) -> Option<Result<ReceivedFrame, Ieee802154Error>> {
        self.driver.received()
    }

    /// Wait until the hardware signals a frame is available in the RX queue.
    pub async fn wait_rx_available(&self) {
        RX_SIGNAL.reset();
        RX_SIGNAL.wait().await;
    }

    /// Enter RX mode explicitly. Needed after channel changes or
    /// initial startup before the first TX triggers `rx_when_idle`.
    pub fn start_receive(&mut self) {
        self.driver.start_receive();
    }
}
