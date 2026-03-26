use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Error as Ieee802154Error;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;

static TX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static RX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub struct Ieee802154Driver<'a> {
    driver: Ieee802154<'a>,
    config: Config,
}

impl<'a> Ieee802154Driver<'a> {
    pub fn new(ieee802154: Ieee802154<'a>, config: Config) -> Self {
        let mut driver = Self {
            driver: ieee802154,
            config,
        };

        driver
            .driver
            .set_rx_available_callback_fn(Self::rx_callback);
        driver.driver.set_tx_done_callback_fn(Self::tx_callback);

        driver.config.rx_when_idle = true;
        driver.update_driver_config(|_| {});

        driver
    }

    pub fn update_driver_config(&mut self, update_fn: impl Fn(&mut Config)) {
        update_fn(&mut self.config);
        self.config.rx_when_idle = true;
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
    pub async fn transmit(&mut self, frame: &[u8]) -> Result<(), Ieee802154Error> {
        TX_SIGNAL.reset();
        self.driver.transmit_raw(frame)?;
        TX_SIGNAL.wait().await;
        Ok(())
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
