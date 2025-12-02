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

        driver.update_driver_config(|_| {});

        driver
    }

    pub fn update_driver_config(&mut self, update_fn: impl Fn(&mut Config)) {
        update_fn(&mut self.config);
        self.driver.set_config(self.config);
    }

    fn rx_callback() {
        RX_SIGNAL.signal(());
    }

    fn tx_callback() {
        TX_SIGNAL.signal(());
    }

    pub async fn transmit(&mut self, frame: &[u8]) -> Result<(), Ieee802154Error> {
        TX_SIGNAL.reset();
        self.driver.transmit_raw(frame)?;
        TX_SIGNAL.wait().await;
        Ok(())
    }

    pub async fn receive(&mut self) -> Result<ReceivedFrame, Ieee802154Error> {
        RX_SIGNAL.reset();
        self.driver.start_receive();

        let frame = loop {
            if let Some(frame) = self.driver.received() {
                break frame;
            }
            RX_SIGNAL.wait().await;
        };

        frame
    }
}
