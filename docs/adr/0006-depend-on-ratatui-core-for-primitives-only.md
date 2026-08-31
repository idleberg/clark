# Depend on ratatui-core for primitives, not on Ratatui as a framework

Having rejected `Terminal` (ADR-0002) and Ratatui's width model (ADR-0005), we re-examined whether
the dependency still earns its place. It does, but only as a primitives library: `Buffer`, `Cell`,
`Style`/`Color`/`Modifier`, and the `Widget` traits. `Layout` and `ratatui-widgets` are not used at
all.

The deciding piece is `BufferDiff` — 664 lines and 22 tests implementing a zero-allocation cell diff
that handles wide-character trailing-cell skipping in both directions, VS16 emoji trailing cells, and
erase ranges where a previous cell was wider than its replacement. This is exactly what the Emitter
needs, it is subtle enough that a hand-rolled version would be wrong in ways only users notice, and
its tests encode terminal pathologies nobody writes down in advance.

## Consequences

`Layout` drags in `kasuari` (a Cassowary solver), `lru` and `hashbrown` for functionality clack has
no analogue of — clack's layout is `String.repeat` and `padEnd` arithmetic. That weight is accepted
knowingly, as the price of the diff.

`ratatui-widgets` is not merely unused but unusable: `Block` draws a different symbol set than
clack's, `Paragraph` wraps under the model ADR-0005 rejects, and `List`'s scrolling differs from
`limitOptions`. A future reader should not expect to find them adopted later.

SGR emission stays ours. The attribute-transition logic lives in `ratatui-crossterm` as a private
`ModifierDiff`, so it cannot be called — though it can be read, and one detail is worth carrying
over: Bold and Dim are both cleared by `NormalIntensity`, so removing one requires reapplying the
other. clack uses both.

## Risk

The whole case rests on `CellDiffOption::ForcedWidth`, which is recent API on a pre-1.0 crate — the
field it replaces is deprecated `since = "0.30.1"`. M1 therefore begins with a probe: place a symbol
whose `fast-string-width` and Ratatui measurements disagree, force the width, diff, and confirm
trailing-cell skipping follows our number. If it does not, this decision reverses and clackatui owns
its own cell grid — roughly 350 lines of stable code, which was the alternative considered and
rejected here.
