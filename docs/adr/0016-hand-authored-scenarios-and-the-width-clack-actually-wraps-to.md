# Hand-authored Scenarios, and the width clack actually wraps to

ADR-0003 makes upstream's tests the specification, and for thirteen `text` Scenarios that has
worked. What it cannot supply is anything upstream never varies. Every harvested Scenario runs at
one terminal size and none of them resizes, so wrapping and re-layout — the two paths `session.rs`
records a divergence in — reached the Grid comparison untested. A harvest cannot find what was never
recorded, so these are written: `scripts/authored/cases.mjs`, run against the same clack by
`scripts/harvest-authored.mjs`.

A hand-authored case is still a Scenario in CONTEXT.md's sense. It holds no expected output. It is a
configuration, a terminal width, and a sequence of keypresses; the bytes are whatever clack writes
when it is run, stored verbatim and never interpreted at record time.

## The width the recording is made at

Finding the right width to record at turned up something about clack that the port had half-noticed
and the Recorder had not. There are two of them:

- `Prompt.render` hard-wraps the whole Frame to **`process.stdout.columns`** — the global, not the
  stream the prompt was handed. So does `restoreCursor`, which is how the cursor gets back over the
  rows that came out.
- `getRows(this.output)` reads the **height** off the prompt's own stream, and `getColumns(output)`
  is used by `note` and `spinner` but not by `text`, which does not wrap its own message at all.

`Emitter::frame` already documents this and takes the two numbers as separate parameters. What was
wrong was upstream of it: the Recorder wrote down `output.columns` — the mock's 80 — and the loader
passed that as the wrap width. Under a test harness the two are different numbers, and the one being
recorded was the one clack ignores. Every recording was therefore made at whatever width the
harvesting terminal happened to be, and nothing said so.

Nothing was actually wrong with the harvested Fixture, because no line in it comes near any
plausible margin. That is luck rather than design, and it stops being luck the moment a Scenario is
written to reach the margin on purpose. So:

- `scripts/recorder/setup.mjs` now pins `process.stdout.columns` to 80 for the duration of each
  recorded test, and writes it down as `terminal.stdout` beside the stream's own numbers. This is
  the one thing that file does *to* the environment rather than observing in it, and it is checked
  by the thing it might have broken: upstream's own snapshots still have to pass, and a re-harvest
  at the pin changed no recorded byte, only added the field.
- `scripts/authored/record.test.mjs` sets it per case and records it the same way.
- The loader wraps by `terminal.stdout` and falls back to `terminal.columns`, so a Fixture recorded
  before any of this was noticed still reads.

## What is missing that the harvest had

The oracle. The first Recorder's evidence that it recorded clack behaving normally is that clack's
own suite passed its own snapshots while it ran (ADR-0010). Nothing here has that, and no amount of
care replaces it. What stands in its place, in descending order of how much it is worth:

1. Every case declares what `text()` must return, and the recorder refuses to write a Fixture if any
   of them resolves to something else. This is not decoration — it caught a miscounted backspace on
   the first run, which would otherwise have been recorded as a perfectly plausible Scenario of
   something nobody meant to test.
2. `the_authored_fixture_records_the_widths_it_claims` checks clack's own bytes against the width the
   Scenario says the terminal was, because the width is *set* rather than observed and nothing else
   downstream would notice if the pin stopped working: the Scenarios would quietly become seven more
   80-column cases and go on passing.
3. Same checkout, same tag as the harvested Fixture, asserted on both.
4. The prompt is called the way upstream's own tests call it, through upstream's own `MockReadable`
   and `MockWritable`, aliased rather than copied.

## What the Scenarios found

They were written to test the port and they hit the tests first. Two bugs, both in code that had
been green for as long as it had existed, because nothing had ever handed it a wide character or a
line that reached the margin:

- `Grid::text` read a wide character twice. `avt` stores one in both the cells it occupies, and the
  helper took `char` from every cell, so the emptiness guard could not find a CJK message that was
  plainly on the terminal. The comparison itself was never affected — it is over whole `Cell`s,
  occupancy included — but the guard was, and a guard that cannot see the message is a guard that
  would have been deleted for being wrong.
- The same guard asked whether the terminal *contains* the message, which stops being true the
  moment the message wraps across rows. It asks for a subsequence now, which survives the break and
  still ties the Grid to this Scenario rather than softening to "something was written".

And one thing they cannot do, which the previous ADR predicted: `every_scenario_draws_clacks_opening_frame`
compares an unwrapped `Frame` against bytes clack wrote post-wrap, which is only sound while nothing
reaches the margin. Two authored Scenarios do. Laying the port's Frame out is `Frame::rows`, which is
the Emitter's and not public, so those two are left to the Grid — counted rather than skipped, so
that "left to the Grid" cannot quietly become "left out".

## The port agreed

All seven pass the Grid comparison — characters, styles and cursor position — on the first run, at
40 and 20 columns, with CJK text, across a wrap that grows and a wrap that shrinks again. That is
worth stating and worth distrusting, so it was mutated: widening the port's wrap by one column fails
the Grid on **four of the seven authored Scenarios and none of the ten harvested ones**. The seven
are carrying coverage the thirteen could not.

## Consequences

- Two Fixtures, one shape, read by `tests/scenarios/mod.rs`. Which one a Scenario came from is a
  question about the evidence behind it, not about what the port owes it, so the tests do not ask.
- Seventeen Scenarios now reach the Grid, up from ten.
- `scripts/harvest-authored.mjs` is deliberate, like `mise run drift` — never CI. It needs the clack
  checkout at the pinned tag, and it says so rather than guessing.
- Resize came next and settled ADR-0014's other divergence, which is written up separately in
  ADR-0017. It needed the Fixture to carry an ordered sequence of *events* rather than a list of
  keypresses, since a resize happens at a point in the byte stream and a keypress does not.
- Not covered, and cheaper to name than to pretend: emoji and ZWJ sequences at a margin. `width.rs`
  models them and `width_parity.rs` checks the model, but no Scenario puts one where it has to
  decide whether to break.
