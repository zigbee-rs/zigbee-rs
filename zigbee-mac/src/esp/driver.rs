use core::task::Poll;
use core::task::Waker;

use embassy_sync::blocking_mutex::CriticalSectionMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_radio::ieee802154::Config;
use esp_radio::ieee802154::Error as Ieee802154Error;
use esp_radio::ieee802154::Ieee802154;
use esp_radio::ieee802154::ReceivedFrame;
use futures_util::Stream;

static TX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static RX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

static RX_WAKER: CriticalSectionMutex<Option<Waker>> = CriticalSectionMutex::new(None);

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
        RX_WAKER.lock(|w| {
            if let Some(w) = w {
                w.wake_by_ref();
            }
        })
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

    #[allow(unused)]
    pub async fn receive_one(&mut self) -> Result<ReceivedFrame, Ieee802154Error> {
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

    pub fn stream<'b>(&'b mut self) -> ReceiverStream<'a, 'b> {
        self.driver.start_receive();
        ReceiverStream { driver: self }
    }
}

pub struct ReceiverStream<'a, 'b> {
    driver: &'b mut Ieee802154Driver<'a>,
}

impl Stream for ReceiverStream<'_, '_> {
    type Item = Result<ReceivedFrame, Ieee802154Error>;

    fn poll_next(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let received = self.driver.driver.received();
        match received {
            Some(frame) => Poll::Ready(Some(frame)),
            None => {
                // SAFETY: lock is called non-reentrantly
                unsafe {
                    RX_WAKER.lock_mut(|w| {
                        *w = Some(cx.waker().clone());
                    });
                }

                Poll::Pending
            }
        }
    }
}
