# A progress bar is a spinner whose message is drawn

M5's fifth unit: `progress`. `progress-bar.ts` is seventy-three lines and none of them write to a
terminal. It makes a [`spinner`](./0032-a-spinner-walks-the-cursor-back-over-a-string-it-never-wrote.md),
keeps a number, and on every `advance` hands that spinner a *message* that happens to be a row of
block characters with the caller's text after it. The frame still turns, the dots still arrive, the
walk back is still measured the way the spinner measures it — and everything this unit owns is the
string in the middle.

So the port is the same shape: [`clark_core::progress`](../../crates/clark-core/src/progress.rs) is
a `Spinner`, three numbers, and a `bar()`.

## The message became a Line

Upstream's bar is `styleText('magenta', …) + styleText('dim', …) + ' ' + msg` — a string with
escapes in it, handed to `spin.message` and written out as-is. A Frame carries no escapes
([ADR-0011](./0011-a-frame-carries-a-style-per-span-and-no-escapes.md)), so what is handed over here
is a `Line`.

That is the one change this unit made to `crate::spinner`: its `message` went from a `String` to a
`Line`, and `start`, `set_message` and `stop` take an `impl Into<Line>` so that every existing
caller still passes a `&str`. Two consequences fell out of it and both were already true upstream:

- `clearPrevMessage` now measures the Line's **text**, not the Line. `wrapAnsi` skips escapes when
  it counts columns, so the rows have always been decided by what is visible; a `Line` says the same
  thing in a form that cannot be measured by accident.
- `removeTrailingDots` runs across the spans rather than over one string, because the regular
  expression does not know the spans are there. A message of `Loading...` still loses its dots with
  a bar in front of it.

## The bar has four colours and only ever uses one

`activeStyle(state)` switches on a `State`: magenta while active, red for `error` and `cancel`,
green for `submit`. `drawProgress` is called from exactly two places — `start` with `'initial'` and
`advance` with `'active'` — and both are the magenta branch. The other three are unreachable, and
not by accident of the corpus: a progress bar's ending is the *spinner's* closing row, which has no
bar on it at all. So the port has one colour and this paragraph, rather than three dead branches and
a `State` parameter nothing varies.

## What `advance` clamps, and what it does not

`value = Math.min(max, step + value)` has a ceiling and no floor. Walk a bar backwards past zero
upstream and `String.prototype.repeat` is called with a negative count and throws. A step here is a
`usize`, which makes that unreachable rather than reproducing it:
[ADR-0013](./0013-the-emitter-diffs-lines-because-clack-does.md) is about defects a *terminal* can
see, and an exception thrown before anything is written is not one.

The arithmetic that decides how much of the bar is filled is kept in floating point —
`((value / max) * size).floor()` — because that is what it is upstream. Integer division would
round the same way most of the time, and "most of the time" is not what the corpus is for.

## Consequences

- **One interval, two renderers.** The thread, the lock and the ending that stops the loop before it
  takes the lock moved out of `crates/clark/src/spinner.rs` into `crates/clark/src/ticker.rs`, generic
  over a three-method `Tick` trait. Two implementations, and the `Drop` that clears a forgotten
  renderer is now written once.
- **The bar characters are a Theme entry**, `S_PROGRESS_CHAR` being three `unicodeOr` calls like
  every other symbol. All three are in the corpus, because a Theme entry no case draws is a Theme
  entry nothing checks.
- **The corpus grew rather than forked.** `scripts/scripted/` was named for scripts of calls and not
  for spinners exactly so that this could be a `kind` in it and an `advance` step alongside
  `message`. Sixty-six scripts now — twenty-four of them bars — and five hundred and sixty-three
  steps.
- **Twenty-five mutants, twenty-three caught first time.** Both survivors were gaps and neither was
  in the port. A `max` left unclamped at zero survived because a bar drawn in the wrong *colour*
  reaches neither test: the byte comparison strips SGR, and the Grid sees only what is still on the
  terminal when the run ends — which, for a spinner, is nothing it drew. The case that kills it is
  wide enough to wrap, so the row the walk back fails to erase stays on the Grid with its colour
  on it. And trimming only the last span survived because no *recorded* message can reach the rest
  of the loop; a `Line` handed straight to `set_message` can, so that one is a unit test.
- **The defect the last ADR is about is worse here**, and recorded: a bar seventeen columns wide in a
  twenty-column terminal is drawn over two rows and walked back over as one — and unlike a message,
  a bar's width is set by an option rather than by whatever the caller happened to say.
