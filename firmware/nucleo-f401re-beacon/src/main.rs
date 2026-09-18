//! A second application on the same board, and the one with hardware attached.
//!
//! No task crate changes to support any of it. `blink` is instantiated twice
//! with different names, pins, counters, gates and periods; the HAL-specific
//! `on_timer` is bound to `TIM3` here - the definition names no interrupt, so
//! which line serves it is the composition's choice.
//!
//! Two serial protocols run at once, which is what this firmware is for now.
//! USART1 receives SBUS by circular DMA through the `stm32f4-uart-dma` tasks,
//! and USART6 speaks MSP DisplayPort to a video transmitter - so the channels
//! the receiver sends come back out in a pilot's goggles.
//!
//! Wiring:
//!
//! | Pin | Signal |
//! | --- | --- |
//! | PA10 | SBUS in, through an external inverter. 100000 baud, 8E2 |
//! | PC6 | DisplayPort out, to the transmitter's serial RX. 115200 8N1 |
//! | PC7 | DisplayPort in, from the transmitter's serial TX |
//!
//! Ground is shared with both. PA5 is the board's LD2; PB0 is an ordinary
//! header pin with no LED on it.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

ferroforge::app! {
    device = stm32f4xx_hal::pac,
    // USART6 was a dispatcher here until the OSD needed the peripheral. A
    // dispatcher is an interrupt vector RTIC borrows for software tasks, so a
    // peripheral raising the same vector would land in the dispatcher instead
    // of its own handler. SPI1 is unused on this board.
    dispatchers = [USART2, SPI1],
    monotonic = Mono,

    use ferroforge_task_blinky::{blink, report};
    use ferroforge_task_msp_displayport::{self as osd, msp::Line, paint};
    use ferroforge_task_stm32f4_timer::on_timer;
    use ferroforge_task_stm32f4_uart_dma as uart_dma;
    // The status task below reads the clock, so these have to be in scope here.
    use rtic_monotonics::{Monotonic as _, fugit::ExtU32 as _};
    use stm32f4xx_hal::{
        dma::{
            DmaChannel, DmaDirection, DmaEvent, Stream7, StreamsTuple,
            traits::Stream as _,
        },
        gpio::{Output, PA5, PB0, PushPull},
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
        // The whole OSD conversation, for the same reason.
        link: osd::Link,
        // Shared rather than local because two tasks need it: the handler that
        // moves bytes, and the one that asks the port to start moving them.
        osd_usart: USART6,
    }

    #[local]
    struct Local {
        heartbeat_led: PA5<Output<PushPull>>,
        beacon_led: PB0<Output<PushPull>>,
        heartbeat_count: u32,
        beacon_count: u32,
        pulse_timer: CounterUs<TIM3>,
        pulse_count: u32,
        // The receive stream lives in the shared `Port`, because reading its
        // cursor is what two tasks need. These are the pieces only one task
        // touches.
        usart: stm32f4xx_hal::pac::USART1,
        tx_stream: Stream7<DMA2>,
        tx_sent: u32,
        sbus_channels: [u16; 16],
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
        Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());

        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let gpiob = cx.device.GPIOB.split(&mut rcc);
        let gpioc = cx.device.GPIOC.split(&mut rcc);
        let mut heartbeat_led = gpioa.pa5.into_push_pull_output();
        let mut beacon_led = gpiob.pb0.into_push_pull_output();
        heartbeat_led.set_low();
        beacon_led.set_low();

        // The HAL-specific task reads and clears this timer's flags; init owns
        // everything else about it.
        //
        // The period has a ceiling the type decides: the task declares
        // `CounterUs<TIM3>`, a 1 MHz counter, and TIM3 is 16-bit, so the longest
        // it can express is 65535 ticks - about 65 ms. Asking for more is
        // `Err(WrongAutoReload)` at runtime, which a build cannot catch.
        let mut pulse_timer = cx.device.TIM3.counter_us(&mut rcc);
        pulse_timer.start(50.millis().into()).unwrap();
        pulse_timer.listen(Event::Update);

        heartbeat::spawn().unwrap();
        beacon::spawn().unwrap();
        status::spawn().unwrap();
        osd_paint::spawn().unwrap();

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

        // USART6 for the goggles: MSP DisplayPort at 115200 8N1, PC6 out and
        // PC7 in.
        //
        // Ordinary interrupt-driven serial rather than DMA. The OSD sends a
        // couple of hundred bytes ten times a second, which a byte interrupt
        // absorbs without noticing - and the DMA UART tasks could not serve
        // this port anyway, because it names `USART1` and `Stream2<DMA2>` as
        // concrete types.
        let osd_serial = Serial::<_, u8>::new(
            cx.device.USART6,
            (
                gpioc.pc6.into_alternate::<8>(),
                gpioc.pc7.into_alternate::<8>(),
            ),
            stm32f4xx_hal::serial::Config::default().baudrate(115_200.bps()),
            &mut rcc,
        )
        .unwrap();
        let (osd_usart, _osd_pins) = osd_serial.release();
        // Received bytes only. Transmission stays masked until there is
        // something to send, because an empty transmit register raises the
        // interrupt continuously.
        osd_usart.cr1().modify(|_, w| w.rxneie().set_bit());

        (
            Shared {
                heartbeat_enabled: true,
                beacon_enabled: false,
                uart: uart_dma::Port::new(rx_stream, ring),
                link: osd::Link::new(),
                osd_usart,
            },
            Local {
                heartbeat_led,
                beacon_led,
                heartbeat_count: 0,
                beacon_count: 0,
                pulse_timer,
                pulse_count: 0,
                usart,
                tx_stream: streams.7,
                tx_sent: 0,
                sbus_channels: [0; 16],
            },
        )
    }

    // Two instances of one definition. They differ in every binding a
    // composition controls - name, priority, resources, gate and period - and
    // share only the definition itself and the telemetry task they report to.
    #[task(
        from = blink,
        priority = 1,
        local = [led = heartbeat_led, count = heartbeat_count],
        shared = [enabled = heartbeat_enabled],
        config = [period_ms: u32 = 250],
        spawn = [report = telemetry],
    )]
    async fn heartbeat(cx: heartbeat::Context) -> !;

    #[task(
        from = blink,
        priority = 2,
        local = [led = beacon_led, count = beacon_count],
        shared = [enabled = beacon_enabled],
        config = [period_ms: u32 = 1000],
        spawn = [report = telemetry],
    )]
    async fn beacon(cx: beacon::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    // The HAL-specific hardware task. Unlike a portable definition it can read
    // and clear the peripheral's update flag, which is what makes the binding
    // actually work rather than merely compile.
    #[task(
        from = on_timer,
        binds = TIM3,
        priority = 3,
        local = [timer = pulse_timer, elapsed = pulse_count],
        spawn = [elapsed = telemetry],
    )]
    fn pulse(cx: pulse::Context);

    // The DMA UART's four tasks. The two receive handlers share one priority
    // because both lock `uart`, and the parser sits below them; the library's
    // documentation says why, and nothing checks it.
    #[task(
        from = uart_dma::on_uart,
        binds = USART1,
        priority = 12,
        shared = [port = uart],
        local = [uart = usart],
        spawn = [frame = parse_frame],
    )]
    fn uart_irq(cx: uart_irq::Context);

    // No spawn: a wrap is not a frame, so this one only watches for the
    // reader being lapped.
    #[task(from = uart_dma::on_rx, binds = DMA2_STREAM2, priority = 12, shared = [port = uart])]
    fn dma_rx(cx: dma_rx::Context);

    #[task(
        from = uart_dma::on_tx,
        binds = DMA2_STREAM7,
        priority = 4,
        local = [stream = tx_stream, sent = tx_sent],
    )]
    fn dma_tx(cx: dma_tx::Context);

    #[task(from = uart_dma::parse, priority = 1, spawn = [decoded = sbus])]
    async fn parse_frame(_cx: parse_frame::Context, bytes: usize);

    // The OSD: one portable definition, this firmware's clock, this firmware's
    // serial port. Ten refreshes a second is fast enough that a stick looks
    // live and slow enough that 115200 keeps up.
    #[task(
        from = paint,
        priority = 2,
        shared = [link = link],
        config = [refresh_ms: u32 = 100],
        spawn = [kick = osd_start_tx],
    )]
    async fn osd_paint(cx: osd_paint::Context) -> !;

    /// What the OSD's `kick` binds to. The library cannot start a transmission
    /// itself: which register does that is a property of this port, not of MSP.
    #[task(priority = 2, shared = [osd_usart])]
    async fn osd_start_tx(mut cx: osd_start_tx::Context, _pending: usize) {
        cx.shared
            .osd_usart
            .lock(|usart| usart.cr1().modify(|_, w| w.txeie().set_bit()));
    }

    /// The goggle port, both directions.
    ///
    /// One handler because the USART has one interrupt, and three short locks
    /// rather than one long one because the two resources are independent. At
    /// this priority - the highest that touches either - RTIC's locks cost
    /// nothing, which is the reason to put the handler above the OSD's own
    /// tasks rather than beside them.
    #[task(binds = USART6, priority = 3, shared = [link, osd_usart])]
    fn osd_uart(mut cx: osd_uart::Context) {
        // One read of DR clears RXNE and every error flag SR just reported, so
        // read it once and work out afterwards what the byte was worth. An
        // overrun costs the byte that was already lost; the MSP parser resyncs
        // on the next `$`.
        let (received, transmit_ready) = cx.shared.osd_usart.lock(|usart| {
            let status = usart.sr().read();
            let received = match status.rxne().bit_is_set() || status.ore().bit_is_set() {
                true => {
                    let byte = usart.dr().read().dr().bits() as u8;
                    status.rxne().bit_is_set().then_some(byte)
                }
                false => None,
            };
            (received, status.txe().bit_is_set())
        });

        let outgoing = cx.shared.link.lock(|link| {
            if let Some(byte) = received {
                link.feed(byte);
            }
            match transmit_ready {
                true => link.next_byte(),
                false => None,
            }
        });

        if transmit_ready {
            cx.shared.osd_usart.lock(|usart| match outgoing {
                Some(byte) => usart.dr().write(|w| w.dr().set(u16::from(byte))),
                // Nothing left to send: mask transmission again, or this
                // handler runs forever on an empty transmit register.
                None => usart.cr1().modify(|_, w| w.txeie().clear_bit()),
            });
        }
    }

    /// Proof of life, once a second, whatever else is happening.
    ///
    /// Without it a silent terminal is ambiguous: the firmware could be running
    /// with nothing to say, or not running at all. `deliveries` counts what the
    /// DMA UART tasks handed on, so a stuck receiver and a mis-framed one look
    /// different from here.
    #[task(priority = 1, shared = [uart, link], local = [last_deliveries: u32 = 0])]
    async fn status(mut cx: status::Context) {
        loop {
            let (deliveries, overruns, errors) = cx
                .shared
                .uart
                .lock(|port| (port.frames, port.overruns, port.errors));
            // `answered` is the one that says whether the goggles are there at
            // all: a transmitter that is connected polls, and one that is not
            // leaves it at zero however well the painting half is working.
            let (refreshes, answered, refused) = cx
                .shared
                .link
                .lock(|link| (link.refreshes, link.answered, link.refused));
            defmt::info!(
                "alive: deliveries={=u32} (+{=u32}) ring-overruns={=u32} uart-errors={=u32}",
                deliveries,
                deliveries.wrapping_sub(*cx.local.last_deliveries),
                overruns,
                errors
            );
            defmt::info!(
                "osd: refreshes={=u32} answered={=u32} refused={=u32}",
                refreshes,
                answered,
                refused
            );
            *cx.local.last_deliveries = deliveries;
            Mono::delay(1000.millis()).await;
        }
    }

    /// SBUS decoding, as an ordinary RTIC task. The DMA UART tasks deliver bytes and
    /// say how many; what they mean is the application's business, so there is
    /// no FerroForge in this one at all.
    ///
    /// A frame is 25 bytes: `0x0F`, 22 bytes holding 16 channels of 11 bits
    /// little-endian across byte boundaries, a flags byte, then `0x00`.
    #[task(
        priority = 1,
        shared = [uart, link],
        local = [sbus_channels, sbus_good: u32 = 0, sbus_bad: u32 = 0, sbus_flags: u8 = 0],
    )]
    async fn sbus(mut cx: sbus::Context, _bytes: usize) {
        let (frame, len) = cx.shared.uart.lock(|port| (port.frame, port.frame_len));
        if len < 25 || frame[0] != 0x0F || frame[24] != 0x00 {
            // Counted rather than printed: a misaligned stream would otherwise
            // produce a line per frame, at SBUS's ~140 Hz.
            *cx.local.sbus_bad = cx.local.sbus_bad.wrapping_add(1);
            if *cx.local.sbus_bad % 100 == 1 {
                defmt::warn!(
                    "sbus: {=u32} unparsable frames, last was {=usize} bytes \
                     starting {=u8:#04x}",
                    *cx.local.sbus_bad,
                    len,
                    frame[0]
                );
            }
            return;
        }

        let mut bits = 0u32;
        let mut held = 0u32;
        let mut channel = 0usize;
        for byte in &frame[1..23] {
            bits |= u32::from(*byte) << held;
            held += 8;
            while held >= 11 && channel < 16 {
                cx.local.sbus_channels[channel] = (bits & 0x07FF) as u16;
                bits >>= 11;
                held -= 11;
                channel += 1;
            }
        }

        *cx.local.sbus_good = cx.local.sbus_good.wrapping_add(1);

        // The goggles. Raw receiver units on screen, deliberately: that is what
        // the log line below prints, and two displays of the same stick
        // disagreeing would be a puzzle worth nobody's time.
        let state = frame[23] & 0x0C;
        cx.shared.link.lock(|link| {
            link.screen.set_row(1, b"FERROFORGE");
            for index in 0..5 {
                let mut row = Line::new();
                row.text(b"CH")
                    .number(index as u16, 1)
                    .number(cx.local.sbus_channels[index], 6);
                link.screen.set_row(3 + index, row.as_bytes());
            }

            let mut condition = Line::new();
            if state & 0x08 != 0 {
                condition.text(b"FAILSAFE");
            } else if state & 0x04 != 0 {
                condition.text(b"FRAME LOST");
            } else {
                condition.text(b"RX OK");
            }
            link.screen.set_row(9, condition.as_bytes());

            // The transmitter asks for channels in microseconds; SBUS counts in
            // its own 11-bit units. This is the conversion every flight
            // controller uses, so its own display agrees with ours.
            for index in 0..osd::msp::poll::CHANNELS {
                link.telemetry.channels[index] =
                    (u32::from(cx.local.sbus_channels[index]) * 5 / 8 + 880) as u16;
            }
        });

        // Once every 50 frames is about three lines a second at SBUS rates -
        // readable, and still fast enough to see a stick move. A change in the
        // failsafe or frame-lost flags prints immediately, because that is the
        // thing you want to notice.
        let flags = frame[23] & 0x0C;
        let changed = flags != *cx.local.sbus_flags;
        *cx.local.sbus_flags = flags;
        if !changed && *cx.local.sbus_good % 50 != 0 {
            return;
        }

        defmt::info!(
            "sbus #{=u32} ch1..4 = {} {} {} {}  failsafe={} lost={} bad={=u32}",
            *cx.local.sbus_good,
            cx.local.sbus_channels[0],
            cx.local.sbus_channels[1],
            cx.local.sbus_channels[2],
            cx.local.sbus_channels[3],
            flags & 0x08 != 0,
            flags & 0x04 != 0,
            *cx.local.sbus_bad
        );
    }
}
