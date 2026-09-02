# Text is measured with a port of fast-string-width, not unicode-width

clack wraps Frames with `fast-wrap-ansi@0.2.0`, which measures using `fast-string-width@3`. Rust's
conventional choice, `unicode-width`, disagrees with it on emoji ZWJ sequences, variation selectors,
regional indicators and combining marks. Since wrap points determine Frame height, and Frame height
determines cursor movement and scrollback, a measurement disagreement is a Parity failure. Both
libraries' semantics are therefore ported into `clark-core`.

## Consequences

This is a deliberate deviation from the idiomatic path, and from what Ratatui itself uses — so a
future reader will be tempted to "fix" it by swapping in `unicode-width`. A Conformance suite feeds a
corpus of emoji, ZWJ, CJK and combining-mark strings to both the JavaScript libraries and the Rust
port, asserting equal widths and equal wrap points, so that swap fails loudly.

Two width models therefore coexist in the process, and they must not be allowed to meet. Ratatui's
own model is not plain `unicode-width` either — `CellWidth for str` adds a correction for halfwidth
Japanese sound marks — so the two agree only by coincidence, never by construction. Consequently:

- `Buffer::set_stringn`, `set_line` and `set_span` are **never** used. They segment graphemes and
  derive cell occupancy from `symbol.cell_width()`, which is Ratatui's model. Cells are placed
  individually instead, at offsets computed by our port.
- Every placed Cell carries `CellDiffOption::ForcedWidth`, so `Buffer::diff_iter` skips trailing
  columns according to our measurement rather than re-deriving its own. This is the mechanism that
  lets us keep Ratatui's diff — see ADR-0006.

A lint or a debug assertion guarding against the `set_string*` family would be cheap insurance.

A related upstream inconsistency is deliberately not reproduced: `Prompt.render()` hard-wraps at
`process.stdout.columns` while every widget lays out at the injected stream's `columns`, so clack
lays out at one width and wraps at another whenever those disagree. The Recorder pins
`process.stdout.columns` to each Scenario's width, making the divergence unobservable, and clark
uses a single width throughout.
