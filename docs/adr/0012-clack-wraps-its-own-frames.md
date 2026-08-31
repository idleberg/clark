# clack wraps its own Frames, so the port wraps them too

ADR-0011 placed one width segment to a cell and left wrapping as an admitted placeholder: a unit
that did not fit moved to the next row whole, on the reasoning that this is what a terminal with
`DECAWM` on does. The reasoning was sound and the premise was wrong. The terminal never gets the
chance.

`@clack/core`'s `render` calls

```js
wrapAnsi(this._render(this) ?? '', process.stdout.columns, { hard: true, trim: false })
```

and writes the result. `restoreCursor` wraps `_prevFrame` the same way and counts rows off it to
decide how far to move the cursor back. So every break in clack's output was computed inside the
process before a byte was written, and the rows are a **word** wrap — text runs on to the next row
at the last space that fits — not the greedy column fill a terminal performs. The two agree exactly
until a line reaches the margin and then differ on every line that does, and because the cursor is
counted off the same rows, a break in the wrong place moves the whole Frame rather than one line of
it.

`fast-wrap-ansi@0.2.0` is therefore ported, in `clackatui-core/src/wrap.rs`, and checked against a
harvested corpus the same way the width port is (ADR-0008): `scripts/harvest-wrap.mjs` runs the real
library, `tests/wrap_parity.rs` asserts against the recording. All 47 cases agree.

## Only one configuration is ported

Upstream's `exec` branches on `trim` and `wordWrap`. clack passes `{ hard: true, trim: false }` and
nothing a Prompt does can reach another combination, so the other branches are left out rather than
carried untested — the module docs list exactly which, so the port can be read against the original.

`trim: false` is what makes the rest of the design work: no space is eaten and no row is trimmed, so
wrapping a line is nothing but a set of positions to break at. That is what `wrap::breaks` returns,
and it is why a Frame can wrap a line without disturbing the spans it is made of.

The escape bookkeeping is left out too. Upstream closes the open SGR code at the end of a row and
reopens it at the start of the next, which is the one place it inserts bytes rather than only
breaking. A Frame holds no escapes at all (ADR-0011); it carries styling as a `Style` per span and
the Emitter re-states that style per cell, so styling survives a break structurally.

## The Frame wraps before it segments, not after

Upstream breaks a word too long for a row by *code point*, measuring each on its own. So a row can
begin part-way through what `width::segments` would call one unit, and the parts are then measured
as the parts they became.

The Frame therefore wraps first and segments each resulting row, which is the order clack has: it
wraps a string, and the terminal draws the rows that come out. Segmenting first and wrapping the
segments would leave the two disagreeing about a row that had already been laid out.

One consequence is visible in `frame.rs`: a unit too wide for the terminal is still given a row of
its own by the wrap, leaving the row it overflowed empty. The Frame draws nothing on that row and
keeps it, because clack counts its cursor back over rows whether or not anything could be placed on
them. ADR-0011 left this to the narrow-terminal Scenarios to adjudicate; it is now upstream's answer
rather than ours.

## `normalize()` reaches further than wrapping

`wrapAnsi` composes its input to NFC before doing anything else, and clack writes what comes back.
This is not a detail of wrapping — it is the last thing that happens to a Frame's text before it
becomes bytes — but it lives here because that is where upstream put it. `unicode-normalization` is
the one Unicode dependency not pinned exactly: composition is stable by Unicode policy, so a table
bump cannot move an answer the way it can for the width tables.

It cost the project its favourite example. `U+1100 U+1161 U+11A8`, the conjoining jamo the M0 probe
was built on because `fast-string-width` measures them as six columns and Ratatui as two, compose to
the single syllable `U+AC01` — which both models measure as two. The disagreement M0 demonstrated is
real, but it cannot arrive at a terminal through clack: `wrapAnsi` precomposes it away first.

That does not weaken ADR-0006 or ADR-0007. The models still part company over emoji sequences (one
cell of two columns under ours, one grapheme under Ratatui's, and *n* under neither when the wrap
breaks one), over tabs (eight columns against one), and over jamo with no composed form. It does
mean the M0 probe would have been better built on a tab, and the Frame's tests now are.

Worth recording plainly: the port found this by disagreeing with the harvest, on one case out of
forty-seven. Nothing in the reasoning would have found it.

## Consequences

- The opening-Frame comparison in `tests/scenario_replay.rs` compares an *unwrapped* Frame against a
  recording that is post-wrap. That is sound only while no line reaches the margin, which at the 80
  columns every harvested Scenario ran in none does — and the test now asserts that rather than
  assuming it. The hand-authored narrow-terminal Scenarios will have to compare rows.
- Those Scenarios can now be written. ADR-0011 said they should wait until the wrap was a port
  rather than a guess, so that they would not be authored against our behaviour; that condition is
  met.
- The truncation half of `fast-string-truncated-width`, left out of the width port with the note
  that it would land when `fast-wrap-ansi` needed it, is still not needed: upstream's wrap uses
  widths, not truncation indices.
