# `restoreCursor` walks back over rows that are not there

ADR-0014 recorded a divergence and said it had no oracle:

> Upstream keeps `_prevFrame` as the *wrapped* string and, on every render, re-wraps it at the
> terminal's current width to count the rows it must walk back over. The Emitter keeps the rows it
> last laid out instead. The two agree whenever the terminal has not narrowed since the previous
> Frame — an already-wrapped row cannot wrap again at the same width or a wider one — and can
> disagree when it has.

The hand-authored resize Scenarios (ADR-0016) are that oracle. Of the four, the two that narrow the
terminal disagreed on the Grid, by exactly one row of vertical offset, and the two that widen agreed.
The prediction was right in both directions.

## What upstream does

`Prompt.render` is inconsistent with itself about how tall the previous Frame was, and the two
answers come from two different places:

- `restoreCursor()` re-wraps `_prevFrame` at **`process.stdout.columns` as it is now** and walks up
  that many rows.
- `diffLines(_prevFrame, frame)` splits `_prevFrame` at **the newlines it was written with**, and
  `numLinesBefore` — the terminal it was actually drawn into — is what the diff offsets use.

While the terminal has not narrowed, those are the same number and nothing turns on it. When it has,
`restoreCursor` walks the cursor further up than the Frame was ever drawn, and everything after is
written one row too high.

The port used its own remembered row count for both, which is the self-consistent reading — and
which puts the output in a different place than clack puts it.

## Why the port follows

Because a terminal can see it. CONTEXT.md defines Parity as clack and clackatui leaving the terminal
in the same observable state, and ADR-0001 makes the Grid the arbiter of what observable means. A
one-row offset is about as observable as a difference gets. The rule this project has followed since
ADR-0013 — reproduce upstream's defects where a terminal can see them, `undefined` and all — applies
here without needing a new argument.

`Emitter::restored` re-wraps the previous rows at the current width; everything else keeps using the
count the rows were drawn at, which is what upstream's diff does. For an unchanged terminal the two
are identical, so nothing that was green went red: 161 unit tests and all 17 pre-existing Scenarios
passed the change untouched.

Whether upstream's behaviour is *right* is a separate question and not this port's to answer. It is
worth noticing that the inconsistency is between two lines of the same function, which is the shape
a bug usually has rather than the shape a decision does. If it is ever fixed upstream, this is one
of the places the Drift check should land.

## What tests it

- Four resize Scenarios in `scripts/authored/cases.mjs`: narrowing under a wrapped value, narrowing
  under a wrapped message, widening until the value fits again, and narrow/widen/narrow so that a
  port which only re-counts once is caught. All four compare on the Grid.
- `a_narrowed_terminal_walks_the_cursor_back_over_rows_that_are_not_there` in `emitter.rs`, which
  pins both directions in eight columns rather than in a whole recording — the unit-level failure
  message for the same fact.

The unit test alone would not be worth much: it was written from the same reading of upstream that
produced the fix, so it can only confirm that the fix does what the fix intended. The Scenarios are
the part that could have said no, and did.

## Consequences

- A Scenario is now an *ordered sequence of events* rather than a list of keypresses. A resize
  records the width, the height, and how many output chunks had been written when it arrived, which
  is what lets both byte streams be cut in the same places and replayed into an emulator that
  changes size with them. A Fixture without an `events` field reads as its keys in order, so the
  harvested recording needed no re-harvest.
- The Grid comparison feeds segments rather than one string. `avt::Vt::resize` is now load-bearing.
- `session.rs` records one divergence rather than two. The one left — `Status::Initial` against
  upstream's shared `state` — is unobservable, which is a different kind of claim and a weaker one:
  it holds only while no Prompt distinguishes `initial` from `active`.
