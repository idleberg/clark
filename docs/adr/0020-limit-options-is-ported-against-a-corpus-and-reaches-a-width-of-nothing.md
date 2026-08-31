# `limitOptions` is ported against a corpus, and reaches a width of nothing

M3 is `select`, `multiselect` and `select-key`, and all three draw their list through one function.
It is ported first, on its own, because it decides which option is under the cursor — a window off
by one is not a difference in decoration but in the answer the user is about to give.

## A corpus, not a recording

`limitOptions` is pure. It takes a list, a cursor, four numbers and a style callback, and returns
rows. There is no Prompt to instrument and no sequence to preserve, so it gets the arrangement
ADR-0008 established for the width and wrap ports rather than the one ADR-0010 established for
Prompts: `scripts/harvest-limit-options.mjs` runs the real function over
`scripts/limit-options/cases.mjs` and writes `fixtures/limit-options.json`, and
`tests/limit_options_parity.rs` asserts against the recording.

Upstream's own suite has fourteen tests. Fifty-four cases are harvested, because the fourteen leave
most of the arithmetic unconstrained: they never vary the terminal's width, never set a column
padding, and never move the cursor through a list one option at a time. Six of seven mutations of
the port are caught by the corpus and none by upstream's fourteen alone.

There is one thing the corpus cannot carry. Every style in it is deliberately plain, so that the
fixture holds no escapes (ADR-0011), which leaves the styling of the `...` overflow row asserted
only by a unit test against the Theme. The Grid comparison will reach it when `select` lands.

## The seventh mutation

Modelling `Array.prototype.splice`'s negative-start clamping — which upstream's two calls could in
principle reach — passed all fifty-four cases. Rather than write a case for it, the reachability was
worked out: the forward walk takes at most `cursorGroup` steps and the backward one at most
`length - cursorGroup - 1`, and `cursorGroup` cannot go below `-1` without a window narrower than
three, which the floor of five forbids. Neither call can run off the array. The model was deleted
and replaced with a subtraction that panics rather than clamps, and the argument is written above
it.

This is the other outcome a mutation can have. M2's surviving mutation was a real gap and produced a
new Scenario; this one was a branch that does not exist, and produced a deletion. Both are the
mutation doing its job.

## A width of nothing

`limitOptions` wraps each option to `columns - columnPadding`, and `select` passes thirteen for the
padding — the length of a styled bar and two spaces, which is ADR-0019's arithmetic in a second
place. So any terminal thirteen columns or narrower arrives at the wrap with nothing left, and a
narrower one with less than nothing.

The wrap port declined to model that. `wrap::breaks` returned no breaks at all for a width of zero,
and its doc comment said a terminal of no columns is not a state a Prompt can be drawn in. That was
true when it was written and is not true now.

What upstream does there is divide by zero. The comparison that decides whether a long word starts
on the next row is between two quotients that are `Infinity` or `NaN` whichever way it lands, so it
is always false; the mid-word break then puts every code point on a row of its own, with a blank row
in front of the first. A negative width is the same case, for the same reason — the two quotients
stay equal. The port now reproduces it, with the one comparison short-circuited rather than
computed, and `columns` staying unsigned: `fixtures/wrap.json` carries a negative width beside a
zero one so that the equivalence is recorded rather than asserted in a comment.

## Wrapping a Line rather than a string

Upstream wraps *styled* strings here: `wrapAnsi` skips escapes when it measures and reopens the
style it closed on the far side of a break. A Frame has no escapes to skip or reopen, so the same
thing is `Line::wrap` — the break offsets `wrap::breaks` already computes, applied to the spans.
The two are the same appearance by construction, and the offsets are `wrap_parity.rs`'s.

`Frame::rows` was already doing this to lay out cells, so the text and style bookkeeping either side
of the break is now shared, and `a_wrapped_line_lays_out_the_way_the_frame_lays_it_out` holds the two
paths together: a Line wrapped and then drawn one row per line reaches the same cells as the same
Line drawn whole. That is what `limit_options` depends on — it counts rows at one width and the
Frame it goes into is wrapped again at another.

It holds with one exception, found by the test failing. Where a single unit is wider than the whole
row, the wrap cannot make it fit and leaves it on a row that is still too wide; wrapping that again
breaks it out a second time. So the wrap is not idempotent, and neither is `fast-wrap-ansi`. Nothing
reaches it — the second width is always the wider one, since the first had a padding taken off it —
and `wrapping_twice_differs_only_where_a_unit_is_wider_than_the_row` pins both halves so that a later
change cannot quietly rely on an idempotence that was never there.

## Consequences

- A fourth harvest script, and the first to reach clack's TypeScript rather than a built package.
  `scripts/limit-options/vitest.config.mjs` aliases the source the way the hand-authored Recorder
  already does.
- `Styles` grows `overflow`. The `...` itself is not a Theme symbol: upstream spells it out rather
  than putting it through `unicodeOr`, so it is three periods in an ASCII terminal too.
- `Line::wrap` is public and `Frame::rows` goes through the same composition. Anything drawing a
  clackatui Line inside its own Ratatui layout can now break it where clack would.
- `wrap::breaks` no longer refuses a width of zero, which is a behaviour change to a ported
  primitive. Every call site that could reach zero was already passing a width it had subtracted
  something from.
