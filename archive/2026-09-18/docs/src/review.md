<!--
Archived from docs/src/review.md, the "Task groups, 2026-09-17" decision
entry, on 2026-09-18, when task groups were removed from FerroForge. Its
binding decision - a library cannot supply task declarations - was promoted
into architecture.md "Firmware Composition" first, and the removal itself is
recorded in review.md as "Groups removed, 2026-09-18". The rest is kept
verbatim and is not design authority.
-->

**Task groups, 2026-09-17.** Some functionality needs several tasks that only
work as a set - a DMA UART needs the USART's IDLE interrupt, both DMA
transfer-complete interrupts and a parser off the interrupt - with rules about
their priorities that no single definition can express.

A library cannot supply the declarations. `#[rtic::app]` parses `mod app` before
any inner `macro_rules!` expands, so a generated `#[task]` is never seen: RTIC
reports "cannot find attribute `task` in this scope". Verified before designing
around it. Task declarations are therefore always lexical, in the firmware.

What is possible splits cleanly. The library owns the shared state as one type,
so the group binds one resource rather than several that could be wired apart.
The library also owns the *rules*, as a trait of associated priority consts with
const assertions. The firmware owns the *numbers*, written where RTIC needs them,
and `#[group]` states `from`, `shared` and a default priority once instead of per
task. `app!` mirrors the chosen priorities into an impl of that trait and forces
it to evaluate - the same mechanism as `config`.

Two properties follow, both verified: a rule the firmware breaks fails in the
library's own words, and a task omitted from the group leaves its const missing,
so an incomplete group is `missing: ON_TX_PRIORITY in implementation` rather than
something that builds and never transmits. This corrects an earlier judgement
that completeness could not be enforced.

Priorities are checked, not propagated. RTIC parses `priority` as a literal, so
"set one and the other follows" is not available; writing both and rejecting a
mismatch gets the same guarantee with the number visible at each task.

