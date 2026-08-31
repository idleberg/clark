# A cell holds one width segment, not one grapheme cluster

ADR-0005 rejects Ratatui's width model and ADR-0006 keeps `BufferDiff` anyway, on the strength of
`CellDiffOption::ForcedWidth`. M0 confirmed the stamp is obeyed. What neither ADR settled is what
goes *into* a stamped cell, and there is only one answer that keeps the two decisions consistent:
the unit of a cell is one unit of the width model that laid the line out.

`fast-string-width` has no notion of a grapheme cluster. It scans six blocks and settles what none
of them claimed, and the answer for a run of text is a sum over those blocks. So the port now hands
that scan back as segments (`width::segments`) rather than only its total, and `width_with` is
defined as the sum of them — one scan, so the layout and the placement cannot come to disagree
about where a block begins.

Two consequences are worth stating because they read like bugs:

- **An emoji sequence is one cell.** A family of four occupies a single cell of two columns, because
  upstream measures the whole sequence as one unit.
- **Conjoining jamo are three cells.** `U+1100 U+1161 U+11A8` render as one syllable block in a
  terminal, and Ratatui measures them as one cluster of two columns. clack measures them per code
  point and gets six. Ours is three cells of two columns each — visibly not what the terminal draws,
  and deliberately so: it is what clack laid the rest of the line out around. This is the M0 probe
  symbol, and the disagreement is the reason it was chosen. (It is also no longer reachable through
  clack, which composes it to one syllable first — see the Consequences below and ADR-0012. The rule
  stands; the illustration does not.)

Segmenting by grapheme cluster instead would produce cells that look right in isolation and a line
whose later units sit at columns clack never used. Parity is measured on the Grid (ADR-0001), where
the second failure is the one that shows.

## Blocks are matched whole, then subdivided

The scan cannot be re-run at each code point, even though the per-code-point blocks would seem to
allow it. `"\u{01}\u{1B}[0m"` is a control run of two characters followed by the latin text `[0m`,
worth three columns; re-matching at the escape finds an ANSI sequence instead and reports zero. So a
block is matched once and then handed out one code point at a time, which is what upstream's scan
does with its accumulator.

## What the Frame does with a segment of no width

An ANSI escape or a control character is dropped. A Frame carries its styling as a `Style` and its
text as text, so an escape appearing in span text is not styling — it is a stray sequence, and
drawing it would put bytes on the Grid that the comparison would then have to explain away. A
combining mark is appended to the cell before it, which is the one case where a cell holds more than
one segment.

## Consequences

- Every cell the Frame writes is stamped, blanks included. A cell left at `CellDiffOption::None`
  would be measured by Ratatui the next time a Frame is diffed against it, and one cell measured
  under the other model is enough to misplace the rest of the row.
- The columns a wide cell covers are left blank. The diff skips them by the forced width, so their
  contents are unobservable — until the cell narrows, which the Emitter has to notice for itself
  (ADR-0007). Nothing here changes that.
- A unit wider than the terminal is left out of the row entirely. A tab is eight columns under this
  model, so a four-column terminal has nowhere to put one. Dropping it is not obviously what a
  terminal does; it is what avoids inventing a column arrangement that every later unit would then
  be measured against.
- **The Frame's wrapping was a placeholder, and is no longer.** It moved a unit to the next row
  whole rather than splitting it at the margin, on the reasoning that this is what a terminal with
  `DECAWM` on does with a wide glyph. The reasoning was sound and the premise was wrong: the
  terminal never gets the chance, because `@clack/core`'s `render` wraps every Frame itself with
  `fast-wrap-ansi` before writing it. **Superseded by ADR-0012**, which ports the wrap, and which
  also reverses the point above — a unit too wide for the terminal now keeps the row upstream gave
  it, and only its contents are dropped.

  One line of this ADR does not survive that port either. `wrapAnsi` composes its input to NFC, so
  the conjoining jamo above are one syllable of two columns by the time anything is placed, and the
  three-cells-of-two-columns example never occurs through clack. The rule it illustrates — a cell
  holds one width segment — is unaffected; see ADR-0012 for what the example cost.
