# A spinner walks the cursor back over a string it never wrote

M5's fourth unit: `spinner`. The first renderer in the port that draws more than once, and the first
with a clock in it. `log`, `intro`, `outro`, [`note`](./0030-a-notes-formatter-returns-a-line.md) and
[`box`](./0031-a-boxs-defaults-are-not-the-documented-ones.md) write once and are finished
([ADR-0029](./0029-the-static-renderers-write-once-and-never-wrap.md)); a spinner writes a row, walks
the cursor back over it, erases it, and writes the next — every eighty milliseconds, until the
caller says stop.

## The clock is an argument, and the corpus is a script

`clark-core` performs no I/O and reads no clock, so [`Spinner`](../../crates/clark-core/src/spinner.rs)
takes the time since `start` as a parameter and returns the bytes a tick would write. Everything that
makes that a *spinner* — the interval, the thread, the lock the caller and the interval share — is in
`clark`, which is the crate that already owns a terminal.

That splits cleanly, but it leaves a recording problem the other three corpora do not have. A
Scenario is a sequence of keypresses and a static case is one call; a spinner is neither. So there is
a fourth Recorder, `scripts/harvest-scripted.mjs`, and a case in it is a **script**: `start`, some
ticks, a `message`, a `stop`. A tick is one `advanceTimersByTime(delay)` against Vitest's fake
clock — with `performance` added to what it fakes, because `formatTimer` reads `performance.now()`
and a recording that did not fake it would carry whatever the machine's real clock said between two
calls. Each step's bytes are written down separately, so a port that goes wrong on the fourth tick
says which tick rather than printing a diff of the whole run.

The name is `scripted` rather than `spinner` because `progress-bar` is a spinner with a bar drawn
into its message and `task-log` is the same shape again; a case here is a script of calls, whatever
is on the other end of them.

## The walk back measures the wrong string

`clearPrevMessage` decides how far up to move the cursor by wrapping `_prevMessage` — the caller's
message — and counting the rows. What was *written* is `${frame}  ${message}${dots}`: three columns
wider, and one row taller whenever those three columns are what pushes it over. In a terminal of
twenty columns, a message of nineteen `x`s is one row by that measurement and two rows on the screen,
so every tick leaves the row above it behind and the spinner marches down the terminal.

Reproduced ([ADR-0013](./0013-the-emitter-diffs-lines-because-clack-does.md)) and recorded on both
sides of the boundary — seventeen columns, nineteen, and twenty. It is the sharpest thing the corpus
holds, because a port that measured the drawn row instead would agree about every character it wrote
and disagree about where they landed.

## Two things upstream says twice, differently

- **`stop()` clears the message.** `_message = msg ?? _message` reads as "keep what you had if given
  nothing", and cannot be: the parameter defaults to `''`, not `undefined`, so the fallback is
  unreachable and `stop()` prints the step symbol and two spaces. `message()` has the same dead
  expression. Both are the plain assignment here.
- **An error is red.** Every Prompt colours `S_STEP_ERROR` yellow, through the `symbol(state)` helper
  the Theme keeps as `Theme::step`. `spinner.ts` spells its own out and picks red. So this module is
  the one place that does not call that helper: it names the symbol and the colour separately,
  because that is the only way to say a thing upstream says two ways.

## `block()` is half a write and half a driver

`start` calls `@clack/core`'s `block()` and `_stop` calls the `unblock` it returned. `block` puts the
input in raw mode and swallows keypresses so that typing during a spinner leaves no echo behind — all
of which belongs to a crate that owns a terminal, and none of which is ported: doing it would mean a
reader thread competing with the caller's own stdin for the rest of the program, which is a larger
promise than a spinner should make. But `block`'s *first* write is `cursor.hide` and `unblock`'s last
is `cursor.show`, and those are on the terminal at exactly those two points. They are written by the
core module with the rest of the writes, and the driver does not write them again.

## Consequences

- **`write_once` grew a sibling.** A tick's row is wrapped to the terminal and has no newline after
  it — the cursor stays at the end of what it drew, because that is what the next tick erases. So
  `emitter::write_wrapped` is `write_once` at a width and without the trailing newline, and
  `write_once` is now that plus a `\n`. The closing row is *not* wrapped: upstream hands that one to
  the terminal whole.
- **The frames are a Theme entry.** `frames` defaults to `unicode ? ['◒','◐','◓','◑'] :
  ['•','o','O','0']`, which is the same branch every symbol in the Theme takes, so it lives with
  them. The `delay` takes that branch too and stays in the driver, being a duration and not a
  drawing.
- **`style_frame` is `Send + Sync`** where a `note`'s and a `box`'s formatters are neither. Not
  tidiness: a spinner is the only renderer here drawn from a thread the caller does not own.
- **In CI the indicator is unreachable on a tick and not on the closing row.** The CI branch writes
  `${frame}  ${msg}...` whatever the indicator says, and `_stop` reads it anyway — so a CI spinner
  with `indicator: 'timer'` shows no time until it stops, and then shows all of it. Recorded.
- **A dropped `Spinner` is cleared, not stopped.** Nothing can know what the ending should have said,
  and leaving the interval running with the cursor hidden is the one outcome that is certainly
  wrong.
- **A corpus of forty-two scripts** — three hundred and seventy-four steps between them, each
  compared as bytes and each whole run compared as a Grid.
- **Thirty-eight mutants, thirty-six caught first time.** Both survivors were gaps in the corpus and
  both were closed by adding a case rather than by changing the port. `removeTrailingDots` widened
  to strip dots from *both* ends survived because no recorded message began with one — a message of
  `...Loading...` closes it. And a second `start` that did not reset the dot counter survived
  because the only restart in the corpus ticked once before stopping, which is too few ticks for a
  dot to appear either way; the restart now runs ten.
