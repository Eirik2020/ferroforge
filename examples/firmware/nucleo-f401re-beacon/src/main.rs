//! A second application on the same board, and the one with hardware attached.
//!
//! `blink` is instantiated twice, as two named copies of one group, with
//! different names, pins, counters, gates and periods; the HAL-specific
//! `on_timer` is bound to `TIM3` here - the definition names no interrupt, so
//! which line serves it is the composition's choice.
//!
//! USART1 receives SBUS by circular DMA through the `stm32f4-sbus` group, which
//! is the `stm32f4-uart-dma` tasks with a decoder behind them.
//!
//! Wiring:
//!
//! | Pin | Signal |
//! | --- | --- |
//! | PA10 | SBUS in, through an external inverter. 100000 baud, 8E2 |
//! | PA11 | SBUS out, a stand-in receiver: wire to PA10 to test without one |
//!
//! Ground is shared with the receiver. PA5 is the board's LD2; PB0 and PA8
//! (Arduino D7) are ordinary header pins with no LED on them.
//!
//! One wire from PA11 to PA10 runs the whole SBUS path on a bare board: the
//! stand-in receiver transmits on USART6, which nothing else here uses, and
//! the SBUS group receives on USART1. `sbus_source` sends frames whose
//! channels are known, and `show` logs what the group decoded, so the two can
//! be compared.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::app! {
    device = stm32f4xx_hal::pac,
    // A dispatcher is an interrupt vector RTIC borrows for software tasks, so
    // a peripheral raising the same vector would land in the dispatcher
    // instead of its own handler. SPI1 and USART2 are unused on this board.
    dispatchers = [USART2, SPI1],

    use rtic_monotonics::systick::prelude::*;

    systick_monotonic!(Mono, 1000);

    use ferroforge_task_blinky as blinky;
    use ferroforge_task_stm32f4_timer::on_timer;
    use ferroforge_task_stm32f4_sbus::{self as sbus, Channels};
    use ferroforge_task_stm32f4_uart_dma as uart_dma;
    use super::sbus_frame;
    use stm32f4xx_hal::{
        dma::{
            DmaChannel, DmaDirection, DmaEvent, Stream7, StreamsTuple,
            traits::Stream as _,
        },
        gpio::{Output, PA5, PA8, PB0, PushPull},
        pac::{DMA2, TIM3, USART6},
        prelude::*,
        rcc::Config,
        serial::{Serial, config::DmaConfig, config::StopBits},
        timer::{CounterUs, Event},
    };

    #[shared]
    struct Shared {
        heartbeat_enabled: bool,
        beacon_enabled: bool,
        // One resource for all the DMA UART tasks, because the library models
        // its state as one type.
        uart: uart_dma::Port,
        // Frames the stand-in receiver sent, and what the SBUS group made of
        // them - good and bad - so the status line can account for every one.
        sbus_sent: u32,
        sbus_seen: (u32, u32),
    }

    #[local]
    struct Local {
        heartbeat_led: PA5<Output<PushPull>>,
        beacon_led: PB0<Output<PushPull>>,
        heartbeat_count: u32,
        beacon_count: u32,
        pulse_timer: CounterUs<TIM3>,
        pulse_count: u32,
        spare_led: PA8<Output<PushPull>>,
        spare_count: u32,
        // The receive stream lives in the shared `Port`, because reading its
        // cursor is what two tasks need. These are the pieces only one task
        // touches.
        usart: stm32f4xx_hal::pac::USART1,
        tx_stream: Stream7<DMA2>,
        sbus_out: USART6,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
        Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());

        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let gpiob = cx.device.GPIOB.split(&mut rcc);
        let mut heartbeat_led = gpioa.pa5.into_push_pull_output();
        let mut beacon_led = gpiob.pb0.into_push_pull_output();
        let mut spare_led = gpioa.pa8.into_push_pull_output();
        heartbeat_led.set_low();
        beacon_led.set_low();
        spare_led.set_low();

        // The HAL-specific task reads and clears this timer's flags; init owns
        // everything else about it.
        //
        // The period has a ceiling the type decides: the task declares
        // `CounterUs<TIM3>`, a 1 MHz counter, and TIM3 is 16-bit, so the longest
        // it can express is 65535 ticks - about 65 ms. Asking for more is
        // `Err(WrongAutoReload)` at runtime, which a build cannot catch.
        let mut pulse_timer = cx.device.TIM3.counter_us(&mut rcc);
        pulse_timer.start(50u32.millis()).unwrap();
        pulse_timer.listen(Event::Update);

        lights::heartbeat_blink::spawn().unwrap();
        lights::beacon_blink::spawn().unwrap();
        spare::blink::spawn().unwrap();
        status::spawn().unwrap();
        sbus_source::spawn().unwrap();

        // USART1 for SBUS: 100000 baud, 8E2, inverted on the wire so an
        // external inverter sits between the receiver and PA10.
        //
        // 8E2 needs `wordlength_9`: on an F4 the parity bit is taken from the
        // word, so nine bits give eight of data plus parity. Asking for
        // `wordlength_8` with parity would silently send seven data bits.
        let serial = Serial::<_, u8>::new(
            cx.device.USART1,
            (
                gpioa.pa9.into_alternate::<7>(),
                gpioa.pa10.into_alternate::<7>(),
            ),
            stm32f4xx_hal::serial::Config::default()
                .baudrate(100_000.bps())
                .wordlength_9()
                .parity_even()
                .stopbits(StopBits::STOP2)
                .dma(DmaConfig::Rx),
            &mut rcc,
        )
        .unwrap();
        // The pins are configured; dropping them does not undo that.
        let (usart, _pins) = serial.release();
        // An idle line is what ends an SBUS frame - there is no length field.
        usart.cr1().modify(|_, w| w.idleie().set_bit());
        // In DMA mode the receive errors are reported through CR3.EIE, not
        // CR1. Without this an overrun sets ORE silently, the USART stops
        // requesting DMA, and reception never resumes.
        usart.cr3().modify(|_, w| w.eie().set_bit());

        // The DMA target. A `static` rather than a field, because the
        // controller needs an address that does not move and RTIC moves its
        // resources into place after `init` returns.
        static mut RING: [u8; uart_dma::RING] = [0; uart_dma::RING];
        // SAFETY: taken once, here, before any task can run.
        let ring: &'static mut [u8; uart_dma::RING] =
            unsafe { &mut *core::ptr::addr_of_mut!(RING) };

        let streams = StreamsTuple::new(cx.device.DMA2, &mut rcc);
        let mut rx_stream = streams.2;
        rx_stream.set_channel(DmaChannel::Channel4);
        rx_stream.set_direction(DmaDirection::PeripheralToMemory);
        rx_stream.set_peripheral_address(usart.dr().as_ptr() as u32);
        rx_stream.set_memory_address(ring.as_ptr() as u32);
        rx_stream.set_number_of_transfers(uart_dma::RING as u16);
        rx_stream.set_memory_increment(true);
        rx_stream.set_peripheral_increment(false);
        // Circular, so reception never stops and never needs restarting.
        rx_stream.set_circular_mode(true);
        rx_stream.listen(DmaEvent::TransferComplete);
        // SAFETY: the stream is fully configured, and `ring` outlives the app.
        unsafe { rx_stream.enable() };

        // `Serial::new` enabled the receiver before the stream existed, so any
        // byte arriving during setup set RXNE and a second would have set ORE -
        // leaving reception wedged before the first frame. Read SR then DR once
        // to discard it and clear every flag it set.
        let _ = usart.sr().read();
        let _ = usart.dr().read();

        // USART6 for the stand-in receiver, transmit only, in SBUS's own
        // framing, on PA11. Polled, so no USART6 interrupt is enabled. The HAL
        // wants a receive pin to build the port, so PA12 is named and the
        // receiver switched off at once: only the transmitter is wanted.
        let sbus_serial = Serial::<_, u8>::new(
            cx.device.USART6,
            (
                gpioa.pa11.into_alternate::<8>(),
                gpioa.pa12.into_alternate::<8>(),
            ),
            stm32f4xx_hal::serial::Config::default()
                .baudrate(100_000.bps())
                .wordlength_9()
                .parity_even()
                .stopbits(StopBits::STOP2),
            &mut rcc,
        )
        .unwrap();
        let (sbus_out, _sbus_pins) = sbus_serial.release();
        sbus_out.cr1().modify(|_, w| w.re().clear_bit());

        (
            Shared {
                heartbeat_enabled: true,
                beacon_enabled: true,
                uart: uart_dma::Port::new(rx_stream, ring),
                sbus_sent: 0,
                sbus_seen: (0, 0),
            },
            Local {
                heartbeat_led,
                beacon_led,
                heartbeat_count: 0,
                beacon_count: 0,
                pulse_timer,
                pulse_count: 0,
                spare_led,
                spare_count: 0,
                usart,
                tx_stream: streams.7,
                sbus_out,
            },
        )
    }

    // One group holding the same group of two twice, as `heartbeat` and
    // `beacon` - a light and the announcer it is wired to inside the group -
    // plus a reporter merged in unprefixed. Every name a copy has is prefixed
    // with its copy's name, so the firmware's resources bind by bare name. The
    // two copies differ in every binding a composition controls - name,
    // priority, resources, gate, period and label - and share only the
    // definitions; each announces itself, so the log tells them apart.
    #[group(
        from = blinky::lights,
        local = [heartbeat_led, heartbeat_count, beacon_led, beacon_count],
        shared = [heartbeat_enabled, beacon_enabled],
        config = [
            heartbeat_period_ms: u32 = 250,
            heartbeat_label: &'static str = "heartbeat",
            beacon_period_ms: u32 = 1000,
            beacon_label: &'static str = "beacon",
        ],
    )]
    mod lights {
        #[task(priority = 1)]
        async fn heartbeat_blink;

        #[task(priority = 1)]
        async fn heartbeat_announce;

        #[task(priority = 2)]
        async fn beacon_blink;

        #[task(priority = 2)]
        async fn beacon_announce;

        #[task(priority = 1)]
        async fn report;
    }

    // The group `lights` holds twice, selected a third time on its own. A
    // group is a definition like a task is, so a firmware may select it as
    // often as it has resources for; the module's name keeps each selection's
    // tasks apart. PA8 is Arduino D7, free for an LED.
    #[group(
        from = blinky::light,
        local = [led = spare_led, count = spare_count],
        shared = [enabled = heartbeat_enabled],
        config = [period_ms: u32 = 500, label: &'static str = "spare"],
    )]
    mod spare {
        #[task(priority = 1)]
        async fn blink;

        #[task(priority = 1)]
        async fn announce;
    }

    // The HAL-specific hardware task. Unlike a portable definition it can read
    // and clear the peripheral's update flag, which is what makes the binding
    // actually work rather than merely compile.
    #[task(
        from = on_timer,
        binds = TIM3,
        priority = 3,
        local = [timer = pulse_timer, elapsed = pulse_count],
        spawn = [elapsed = lights_report],
    )]
    fn pulse(cx: pulse::Context);

    // SBUS in and the DMA UART's transmit side, selected as one set: a group
    // of groups from two crates. `sbus_link` is `sbus_rx` plus the UART's
    // transmit group, and `sbus_rx` is the UART's receive group plus the SBUS
    // decoder, wired to it by the SBUS crate. This binds the union of their
    // resources once and declares each of the five tasks as RTIC declares a
    // task. The two receive handlers share one priority because both lock
    // `uart`, and the parser and decoder sit below them; the libraries'
    // documentation says why, and nothing checks it.
    #[group(
        from = sbus::sbus_link,
        shared = [port = uart],
        local = [uart = usart, stream = tx_stream],
        spawn = [channels = show],
    )]
    mod sbus_link {
        #[task(binds = USART1, priority = 12)]
        fn on_uart;

        // No spawn: a wrap is not a frame, so this one only watches for the
        // reader being lapped.
        #[task(binds = DMA2_STREAM2, priority = 12)]
        fn on_rx;

        #[task(binds = DMA2_STREAM7, priority = 4)]
        fn on_tx;

        #[task(priority = 1)]
        async fn parse;

        #[task(priority = 1)]
        async fn decode;
    }

    /// Proof of life, once a second, whatever else is happening.
    ///
    /// Without it a silent terminal is ambiguous: the firmware could be running
    /// with nothing to say, or not running at all. `deliveries` counts what the
    /// DMA UART tasks handed on, so a stuck receiver and a mis-framed one look
    /// different from here.
    #[task(
        priority = 1,
        shared = [uart, sbus_sent, sbus_seen],
        local = [last_deliveries: u32 = 0],
    )]
    async fn status(mut cx: status::Context) {
        loop {
            let (deliveries, overruns, errors, head) = cx
                .shared
                .uart
                .lock(|port| (port.frames, port.overruns, port.errors, port.head()));
            // `ring-head` is where the DMA controller will write next. It moves
            // with every byte USART1 receives, whether or not an idle line
            // ever ends a frame, so it tells no signal apart from no frames.
            defmt::info!(
                "alive: deliveries={=u32} (+{=u32}) ring-overruns={=u32} uart-errors={=u32} ring-head={=usize}",
                deliveries,
                deliveries.wrapping_sub(*cx.local.last_deliveries),
                overruns,
                errors,
                head
            );
            // Every frame accounted for: with the jumper fitted, `missing` is 0
            // or 1 - a frame can be on the wire as this reads - and `bad` is 0.
            let sent = cx.shared.sbus_sent.lock(|count| *count);
            let (good, bad) = cx.shared.sbus_seen.lock(|seen| *seen);
            defmt::info!(
                "sbus: sent={=u32} decoded={=u32} bad={=u32} missing={=u32}",
                sent,
                good,
                bad,
                sent.wrapping_sub(good.wrapping_add(bad))
            );
            *cx.local.last_deliveries = deliveries;
            Mono::delay(1000u64.millis()).await;
        }
    }

    /// A stand-in SBUS receiver, so the group can be tested on a bare board:
    /// one frame every 14 ms, as a receiver sends them, out of PA11.
    ///
    /// Channel 1 counts, as `172 + sent % 1640`, and channels 2 to 4 are fixed
    /// at 992, 172 and 1811, so a frame decoded intact is recognisable in
    /// `show`'s log line: with nothing lost, channel 1 there is
    /// `172 + good % 1640`. Polled, a byte at a time; a frame is 25 bytes of 12
    /// bits at 100000 baud, 3 ms at the lowest priority, so with the 14 ms wait
    /// about 59 frames a second.
    #[task(priority = 1, shared = [sbus_sent], local = [sbus_out, sent: u32 = 0])]
    async fn sbus_source(mut cx: sbus_source::Context) {
        loop {
            *cx.local.sent = cx.local.sent.wrapping_add(1);
            let sent = *cx.local.sent;
            let mut channels = [0u16; 16];
            channels[0] = 172 + (sent % 1640) as u16;
            channels[1] = 992;
            channels[2] = 172;
            channels[3] = 1811;
            for (index, channel) in channels.iter_mut().enumerate().skip(4) {
                *channel = 1000 + index as u16;
            }
            for byte in sbus_frame(&channels) {
                while cx.local.sbus_out.sr().read().txe().bit_is_clear() {}
                cx.local.sbus_out.dr().write(|w| w.dr().set(u16::from(byte)));
            }
            // Counted only once the whole frame is out, so a frame is never
            // counted as sent before it could have been received.
            cx.shared.sbus_sent.lock(|count| *count = sent);
            Mono::delay(14u64.millis()).await;
        }
    }

    /// What the channels are for, as an ordinary RTIC task: the SBUS group
    /// decodes, and this firmware logs them.
    #[task(
        priority = 1,
        shared = [sbus_seen],
        local = [flags: (bool, bool) = (false, false)],
    )]
    async fn show(mut cx: show::Context, update: Channels) {
        cx.shared
            .sbus_seen
            .lock(|seen| *seen = (update.good, update.bad));
        // Once every 50 frames is about three lines a second at SBUS rates -
        // readable, and still fast enough to see a stick move. A change in the
        // failsafe or frame-lost flags prints immediately, because that is the
        // thing you want to notice.
        let flags = (update.failsafe, update.lost);
        let changed = flags != *cx.local.flags;
        *cx.local.flags = flags;
        if !changed && !update.good.is_multiple_of(50) {
            return;
        }

        defmt::info!(
            "sbus #{=u32} ch1..4 = {} {} {} {}  failsafe={} lost={} bad={=u32}",
            update.good,
            update.values[0],
            update.values[1],
            update.values[2],
            update.values[3],
            update.failsafe,
            update.lost,
            update.bad
        );
    }
}

/// One SBUS frame: `0x0F`, sixteen 11-bit channels packed least significant
/// bit first into 22 bytes, a flags byte with nothing set, then `0x00`.
fn sbus_frame(channels: &[u16; 16]) -> [u8; 25] {
    let mut frame = [0u8; 25];
    frame[0] = 0x0F;
    let mut bits = 0u32;
    let mut held = 0u32;
    let mut index = 1;
    for channel in channels {
        bits |= u32::from(*channel & 0x07FF) << held;
        held += 11;
        while held >= 8 {
            frame[index] = bits as u8;
            index += 1;
            bits >>= 8;
            held -= 8;
        }
    }
    frame
}
