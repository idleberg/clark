# ForcedWidth holds, but the Emitter owns shrink repaints

M0, the probe ADR-0006 made its case conditional on, has run. `crates/clark-core/tests/forced_width_probe.rs`
is the experiment; this records what it found.

**`CellDiffOption::ForcedWidth` does control trailing-column skipping.** A cell stamped with our
width is followed by exactly that many skipped columns, in both directions — forcing a width larger
than Ratatui's measurement hides the columns underneath it, forcing a smaller one exposes them.
`Buffer::diff_iter` never re-derives its own number for a stamped cell. ADR-0006 stands and
`BufferDiff` is ours to reuse.

## Consequences

One gap the probe was not looking for, and it constrains the Emitter.

**Shrinking a forced-wide cell does not yield the columns it vacated.** The `ForcedWidth` arm of the
diff advances past the forced width and returns; unlike the `CellDiffOption::None` arm, it builds no
trailing range. So when a Frame replaces a 6-column cell with a 1-column one, columns 1–5 are never
emitted and the terminal keeps showing the old glyph.

The `None` arm is not an escape hatch. Its force-trailing branch is guarded on the previous cell
having carried a background colour or a modifier visible on a blank cell — an unstyled wide cell
fails that guard and the columns stay unpainted just the same. Ratatui can afford the guard because
printing a wide glyph physically clears its own trailing columns, so only style can be left stale.
Under a forced width that reasoning does not hold: our width is the one the layout used, not the one
the terminal advances the cursor by.

~~**The Emitter must therefore track shrunk ranges itself** and mark every vacated column dirty
before diffing, rather than relying on the diff to notice. This is a handful of lines, not a
redesign, but it has to exist before the first Frame with variable-width content is reconciled —
which is M1.~~

**Superseded by ADR-0013**, which found the premise wrong rather than the finding. The Emitter does
not diff cells at all: clack compares Frames line by line and erases a row before rewriting it, so
no column is ever left holding an older, wider glyph and there is nothing to mark dirty. The gap
above is real and still applies to anyone who diffs stamped cells — which the `Widget` impl's users
do, since it is what makes a clark Prompt drawable inside someone else's Ratatui application.

## An unrelated finding, recorded because it will mislead someone

ADR-0005 justifies porting `fast-string-width` by listing where `unicode-width` disagrees with it:
emoji ZWJ sequences, variation selectors, regional indicators, combining marks. Measured against
`fast-string-width@3.0.2` and `unicode-width` 0.2.2, **every one of those now agrees** —
`unicode-width` 0.2.2 implements UAX #51. The disagreements that remain are conjoining Hangul jamo
(`U+1100 U+1161 U+11A8`: 6 versus 2, which is the probe symbol), a lone ZWJ (1 versus 0), and
control characters, where Ratatui's `str::cell_width` fires a debug assertion rather than returning
a width.

This does not reverse ADR-0005 — the ADR's own argument is that the two models agree by coincidence
and never by construction, which is as true now as it was. It does mean the width port buys less
than the roadmap implies, and that a reader who checks the ADR's emoji examples will find them
passing and wrongly conclude the port is unnecessary. The Conformance suite ADR-0005 calls for
should carry the jamo case, not the emoji cases, as its load-bearing example.
