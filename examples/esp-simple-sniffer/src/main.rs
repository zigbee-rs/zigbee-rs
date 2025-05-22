#![no_std]
#![no_main]

use core::fmt::Display;
use core::str;

use byte::TryRead;
use esp_backtrace as _;
use esp_hal::esp_riscv_rt::entry;
use esp_ieee802154::*;
use esp_println::println;
use ieee802154::mac::Address;
use zigbee::nwk::frame::Frame as NwkFrame;
use zigbee::security::SecurityContext;

#[entry]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut ieee802154 = Ieee802154::new(peripherals.IEEE802154, peripherals.RADIO_CLK);

    ieee802154.set_config(Config {
        channel: 11,
        promiscuous: true,
        rx_when_idle: true,
        auto_ack_rx: false,
        auto_ack_tx: false,
        ..Config::default()
    });

    println!("Start receiving:");
    ieee802154.start_receive();

    loop {
        if let Some(Ok(frame)) = ieee802154.received() {
            print_ieee802154_mac_frame(&frame).and_then(print_zigbee_nwk_frame);
        }
    }
}

fn print_ieee802154_mac_frame(frame: &ReceivedFrame) -> Option<&[u8]> {
    let frame = &frame.frame;
    let header = frame.header;

    //println!(
    //    "[IEEE 802.15.4] [{frame_type:?}] [Dest: {}] [Src: {}] [Payload size:
    // {size}B]",    PrintableAddress(header.destination),
    //    PrintableAddress(header.source),
    //    frame_type = frame.content,
    //    size = frame.payload.len(),
    //);

    match frame.content {
        ieee802154::mac::FrameContent::Data => Some(&frame.payload),
        _ => None,
    }
}

fn print_zigbee_nwk_frame(payload: &[u8]) -> Option<&[u8]> {
    let mut buf = [0u8; 256];
    hex::encode_to_slice(payload, &mut buf[0..(payload.len() * 2)]).unwrap();
    let s = unsafe { str::from_utf8_unchecked(&buf) };

    let (nwk_frame, _) = NwkFrame::try_read(payload, SecurityContext::no_security()).unwrap();
    match nwk_frame {
        NwkFrame::Data(nwk_data_frame) => {
            //println!(
            //    "  [ZIGBEE] [NWK] [Security Header: {:?}]",
            //    nwk_data_frame.aux_header
            //);
            //println!(
            //    "  [ZIGBEE] [NWK] [Data] [Security: {security}] [Payload size: {size}B]",
            //    security = nwk_data_frame.header.frame_control.security_flag(),
            //    size = nwk_data_frame.payload.len(),
            //);

            None
        }
        NwkFrame::NwkCommand(nwk_command_frame) => {
            let method = nwk_command_frame.header.frame_control.transmission_method();
            //println!(
            //    "  [ZIGBEE] [NWK] [Security Header: {:?}]",
            //    nwk_command_frame.aux_header
            //);
            println!("  [ZIGBEE] payload: {s}");
            println!("  [ZIGBEE] header: {:?}", nwk_command_frame.header);
            println!(
                "  [ZIGBEE] [NWK] [Command] [Security: {security}] [{cmd_type:?}] [{method:?}]",
                security = nwk_command_frame.header.frame_control.security_flag(),
                cmd_type = nwk_command_frame.command,
            );

            None
        }
        NwkFrame::Reserved(_) => {
            println!("  [ZIGBEE] [NWK] [Reserved]");
            None
        }
        NwkFrame::InterPan(_) => {
            println!("  [ZIGBEE] [NWK] [InterPan]");
            None
        }
    }
}

#[derive(Debug)]
struct PrintableAddress(Option<Address>);

impl Display for PrintableAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(addr) = self.0 {
            match addr {
                Address::Short(pid, short_address) => {
                    write!(f, "0x{:04X} (PAN: 0x{:04X})", short_address.0, pid.0)
                }
                Address::Extended(pid, extended_address) => {
                    write!(f, "0x{:016X} (PAN: 0x{:04X})", extended_address.0, pid.0)
                }
            }
        } else {
            write!(f, "None")
        }
    }
}

//fn format_address(address: Option<&Address>) -> [u8; 8] {
//    use byte::TryWrite;
//    let mut buf = [0u8; 8];
//    if let Some(address) = address {
//        let _ = match *address {
//            Address::Short(_, addr) => format_args!("{}", addr.0),
//            Address::Extended(_, addr) => format_args!("{}", addr.0),
//        };
//    }
//    buf
//}
