# The Emitter diffs lines, because clack does

ADR-0002 decided clackatui owns its inline Emitter rather than using `ratatui::Terminal`, and that
decision holds — `Viewport::Inline` still cannot change height, and clack's Frames still change
height constantly. But the ADR went one sentence further than it had evidence for:

> The Emitter is not writing a diff from scratch: it consumes `Buffer::diff_iter` and is responsible
> only for turning those cell updates into cursor movement, erasure and SGR transitions.

That is not what upstream does. `@clack/core`'s `render` compares the previous Frame with the new
one **line by line** — `diffLines` returns the indices of the lines that are not string-equal — and
then rewrites whole lines. Every movement it makes is derived from line counts, never from a column
a cell happens to sit at.

## The branch is observable, so it has to be the same branch

There are three exits, and they leave the cursor in three different places:

| what changed | what is written | where the cursor ends |
|---|---|---|
| nothing | nothing at all | wherever it was |
| exactly one row | walk back, step down to it, `ESC[2K`, the row, step back down | the end of *that* row's text |
| more than one row | walk back, step down to the first, `ESC[J`, every row from there | the end of the last row |

A cell diff would repaint the same characters and land the cursor somewhere else. Cursor position is
part of the Grid (CONTEXT.md), and the Grid is what a parity claim is about (ADR-0001) — so an
Emitter that reconciled cells correctly would still be wrong, and would be wrong in a way that only
showed up as an unexplained mismatch in some later Scenario. `crates/clackatui-core/src/emitter.rs` is
therefore a port of `render`, in the same sense that `wrap.rs` is a port of `fast-wrap-ansi`.

`scripts/harvest-emitter.mjs` is its oracle, and it does not reimplement anything: it constructs a
real `Prompt` whose `render` option returns the next frame from a list and whose `output` collects
writes, then steps it. What lands in `tests/fixtures/emitter.json` is what clack put on the wire.
All 40 cases agree, byte for byte.

Byte equality is stricter than the Grid parity the project claims, and is asked for here on purpose:
the output is nearly all cursor arithmetic, where a Grid comparison is exactly the wrong instrument.
The corpus is colourless so that the strictness is fair — matching clack's styling bytes would be
asserting picocolors' encoding, not clack's algorithm. Styling is asserted through the Grid, where
it belongs.

## The diff is over the Frame's rows, not a Buffer's

A `Buffer` row is padded to the terminal width, so a line that ends in a space and a line that ends
where the padding begins are the same row in a `Buffer` and two different strings to clack. Diffing
`Frame::rows` — the wrapped rows, before anything is placed into a `Buffer` — keeps the comparison
exactly as discriminating as upstream's.

This puts `CellDiffOption::ForcedWidth` off the Emitter's path entirely. It is worth being plain
about what that does and does not cost M0. The probe's finding stands and is still load-bearing: the
`Widget` impl is what makes a clackatui Prompt drawable inside someone else's Ratatui application,
which is half of ADR-0002's case, and it is stamped cells that make that drawing land on clack's
columns. What is no longer reachable is ADR-0007's constraint — "the Emitter must therefore track
shrunk ranges itself" — because clack erases a row before rewriting it and never leaves a column
holding an older, wider glyph. That paragraph was written about an Emitter that does not exist.

## One defect is reproduced rather than fixed

When a Frame loses exactly its last row, upstream's single-line branch indexes past the end of its
own array: `lines[diffLine]` is `undefined`, `output.write` stringifies it, and the terminal is sent
the eight characters `undefined`. It is reproducible in two frames — `"a\nb\nc"` followed by
`"a\nb"` — and the harvest records it verbatim.

The port does the same thing. Not because the output is defensible, but because the alternative is
worse: an Emitter that writes a blank row there would disagree with every harvested Fixture that
reaches the path, and the conformance suite would need an exception list — at which point the oracle
has stopped being an oracle and the project's one real safeguard becomes a matter of opinion. The
deviation is a one-line change if it is ever wanted, and `emitter_parity.rs` pins the upstream
behaviour by name so that a fix upstream is reported as Fixture Drift rather than as a mystery.

It should be reported to clack.

## Consequences

- ADR-0002's `diff_iter` sentence is superseded. Its decision — own the Emitter — is not; if
  anything this strengthens it, since `Terminal` could not have produced these sequences either.
- ADR-0007's shrink-repaint requirement is unreachable on the Emitter path and no longer needs
  implementing. Its finding about `diff_iter` remains true of the `Widget` path.
- The Emitter produces bytes and does not write them, so `clackatui-core` stays free of I/O. The
  driver in `clackatui` is what puts them on a terminal, and is what will decide `columns` and
  `rows` — two numbers, because upstream reads the wrap width from the global `process.stdout` and
  the height from the Prompt's own output stream.
- With the Emitter in place, a recorded Fixture can be replayed as bytes through an emulator and
  compared as a Grid, which is what M1 finishes with. What is still missing between here and there
  is `.interact()`.
