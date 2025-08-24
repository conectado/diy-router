#![no_main]
#![no_std]

use cortex_m_semihosting::hprintln;
use embedded_hal_bus::spi::ExclusiveDevice;

#[cfg(not(debug_assertions))]
use panic_halt as _;

#[cfg(debug_assertions)]
use panic_semihosting as _;

use stm32f4xx_hal::{self as hal, hal::spi::SpiDevice};

use crate::hal::{pac, prelude::*, spi};
use cortex_m_rt::entry;

mod enc28j60;
use enc28j60::Enc28j60;

#[entry]
fn main() -> ! {
    let p = pac::Peripherals::take().unwrap();

    let gpioa = p.GPIOA.split();

    let mut spi_nss = gpioa.pa4.into_push_pull_output();
    spi_nss.set_high();

    let spi_sck = gpioa.pa5;
    let spi_miso = gpioa.pa6;
    let spi_mosi = gpioa.pa7;
    let mut rcc = p.RCC.constrain().cfgr.freeze();

    let mut enc28j60 = Enc28j60::<50, 50>::with_erx_length((0x1f0u16).try_into().unwrap());

    let spi = spi::Spi::new(
        p.SPI1,
        (spi_sck, spi_miso, spi_mosi),
        spi::Mode {
            polarity: spi::Polarity::IdleLow,
            phase: spi::Phase::CaptureOnFirstTransition,
        },
        1.MHz(),
        &mut rcc,
    );

    let mut spi_device = ExclusiveDevice::new_no_delay(spi, spi_nss).unwrap();

    enc28j60.init().unwrap();

    while let Some(mut transaction) = enc28j60.poll_pending_transaction() {
        {
            let mut spi_transaction = heapless::Vec::<_, 3>::from_iter(
                transaction
                    .iter_mut()
                    .map(embedded_hal::spi::Operation::from),
            );
            spi_device
                .transaction(spi_transaction.as_mut_slice())
                .unwrap();
        }

        enc28j60.handle_transaction(transaction);
    }

    loop {}
}
