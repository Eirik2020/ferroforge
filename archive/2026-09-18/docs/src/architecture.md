<!--
Archived from docs/src/architecture.md, the "Groups" subsection of
"Firmware Composition", on 2026-09-18, when task groups were removed from
FerroForge. The binding decision it held - a library cannot supply task
declarations, because #[rtic::app] parses mod app before any inner macro
expands - was promoted into "Firmware Composition" first. The rest is kept
verbatim and is not design authority.
-->

### Groups

Some functionality needs several tasks that only work as a set. A DMA UART needs
the USART's IDLE interrupt, both DMA transfer-complete interrupts and a parser
off the interrupt; three of those touch one ring buffer and must not preempt each
other. A firmware cannot be expected to know that, and a single task definition
cannot express it.

A group block states once what would otherwise be repeated, and names the trait
its library uses to say how the priorities must relate:

```rust,ignore
#[group(from = uart_dma, wiring = uart_dma::Wiring, shared = [port = uart], priority = 12)]
mod serial {
    #[task(from = on_uart, binds = USART1, local = [uart = usart])] fn uart_irq(cx);
    #[task(from = on_rx, binds = DMA2_STREAM2)] fn dma_rx(cx);
    #[task(from = on_tx, binds = DMA2_STREAM7, priority = 4, shared = [], local = [stream = tx])] fn dma_tx(cx);
    #[task(from = parse, priority = 1)] async fn parse_frame(cx, bytes: usize);
}
```

`from` inside the block is relative to the group's. Its `shared` and `priority`
are defaults each task may override; `shared = []` is how a task with no shared
state opts out. Everything else stays per-task, and every interrupt binding stays
visible, because RTIC requires the declarations to be lexically present.

The library states the rules, and gets to word its own errors:

```rust,ignore
pub trait Wiring {
    const ON_UART_PRIORITY: u8;
    const ON_RX_PRIORITY: u8;
    const ON_TX_PRIORITY: u8;
    const PARSE_PRIORITY: u8;

    const CHECK: () = {
        assert!(Self::ON_UART_PRIORITY == Self::ON_RX_PRIORITY,
            "on_uart and on_rx both lock Port from interrupt context; ...");
        assert!(Self::PARSE_PRIORITY < Self::ON_RX_PRIORITY,
            "parse exists to run after the receive interrupts ...");
    };
}
```

`app!` mirrors the chosen priorities into an implementation and forces it to
evaluate - the same mechanism as `config`, which is also an associated-const
trait. Two things follow. A rule the firmware breaks fails in the library's own
words, and a task left out of the group leaves its const missing, so an
incomplete group is `missing: ON_TX_PRIORITY in implementation` rather than
something that builds and never transmits.

What a group cannot do is propagate a priority: RTIC parses `priority` as a
literal, and a macro cannot supply task declarations from a library at all,
because `#[rtic::app]` parses `mod app` before any inner macro expands. So the
numbers are written per task and checked, not inherited.

