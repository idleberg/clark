# A box's defaults are not the documented ones

M5's third unit: `box`. It draws the same thing a [`note`](./0030-a-notes-formatter-returns-a-line.md)
draws and arrives at it from the opposite end. A `note` measures its content and fits a box around
it; a `box` settles on a width first — from the terminal, or a fraction of it — and then makes the
content fit. That inversion is why this one truncates its title, aligns its rows, and needs its own
arithmetic rather than a parameter on the other.

## Two options do the opposite of what their docs say

Both were found by the corpus rather than by reading, and both are reproduced (ADR-0013). They are
the same bug written twice: an option read through `opts?.x` where the documented default is truthy.

- **`width`.** Documented as "`'auto'` to fit the content or a number for a fixed width",
  `@default 'auto'`. The shrink-to-content branch is guarded by `opts?.width === 'auto'`, which an
  omitted option does not satisfy — so the default fills the terminal, and the documented default is
  only reachable by asking for it in as many words. The number is worse: it is
  `Math.floor(columns * Math.min(1, opts.width))`, a *fraction*, so `width: 40` is not forty columns
  but all of them.
- **`rounded`.** Documented `@default true`, read as `opts?.rounded ? roundedSymbols : squareSymbols`.
  A `box()` with no options has square corners.

[`Width`](../../crates/clackatui-core/src/box.rs) therefore has three variants where upstream's type has
two: `Full` is what an omitted `width` does, `Auto` is what the string `'auto'` does, and `Fraction`
is what the number does. `Options::default()` says `rounded: false`. The names are the only place in
this port where the difference is written down, which is the point of them.

## The arithmetic is signed

Upstream computes in doubles and several of these quantities go negative in a narrow terminal — a
title padded wider than the box it sits in, a content padding with no room left for content. Where a
negative reaches `String.prototype.repeat` upstream throws a `RangeError` and the box is never
written at all; a thrown exception leaves nothing on a terminal for a Grid to be faithful to, so the
port computes in `isize` and clamps at the `repeat`. Every case in the corpus is one that upstream
completed.

## `slice` is by UTF-16 code unit, and the truncation test is by column

`truncatedTitle` is `stringWidth(title) > maxTitleLength ? title.slice(0, maxTitleLength - 3) + '...'
: title` — the *decision* counts columns and the *cut* counts code units. A title of wide characters
is therefore cut somewhere a width-counting port would not cut it, and `slice_utf16` exists to cut it
in the same place. Two corpus cases exist for it: one whose title ends in CJK characters, and one of
emoji where the cut falls inside a surrogate pair.

The negative index the same expression can produce (`maxTitleLength` below three, which JS reads as
"count back from the end") is written out and unrecordable: reaching it requires a title wider than
`maxTitleLength`, and every such title is then wider than the box it has to be padded into, so
upstream throws before it writes. Kept because it is what the expression says, and covered by a unit
test rather than by the corpus.

## Consequences

- **Nothing a `box` draws is styled.** `note` draws its border in the Guide's gray; `box.ts` calls
  `styleText` nowhere, including on the bar in the left margin. The only colour a box can have is
  what `format_border` puts there, and upstream's default for that is the identity. No new Theme
  entries — the symbols were all already there, as they were for `note`.
- **`format_border` returns a `Line`**, for the reason ADR-0030 gives about `note`'s `format`.
  Upstream applies it to each distinct border character *before* the repeats, so a formatter that
  returns an escape has it reopened per column and one that returns characters makes the border wider
  than the box the widths were computed for. Both are in the corpus.
- **The box is always an even number of columns wide.** An odd width is nudged up if there is room to
  the right and down if there is not. Nothing upstream explains it; two cases record it either way.
- **The title's padding is border characters and the content's is spaces.** `titlePadding: 1` puts one
  `─` to the title's left, not one space.
- **Thirty-two mutants, thirty-one caught.** The survivor is equivalent rather than untested:
  `longest + 2 < boxWidth` widened to `<=` changes nothing, because on a tie the assignment it
  guards writes the value that is already there. The one that did start out surviving was the
  code-unit slice above — a title of CJK characters cannot tell it from a code-point one, since both
  are one unit each, so the corpus gained a title of emoji where the cut falls *inside* a surrogate
  pair. clack writes the lone half, Node encodes it as `U+FFFD`, and the port lands on the same
  replacement character and the same column.
- **The corpus grew from forty-four cases to eighty-seven**, and a case's options now reach the parity
  test as raw JSON rather than as a field apiece — `box` has seven of them.
