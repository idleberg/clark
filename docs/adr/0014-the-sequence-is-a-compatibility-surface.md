# The order a Prompt is asked in is a compatibility surface, so it lives in the core

`.interact()` needs three things that already existed — a `Prompt`, a widget that draws it, and an
Emitter — plus a terminal. The obvious shape is a loop in the `clackatui` crate that reads a key,
feeds the Prompt, draws, and writes. That shape is wrong in one respect, and the recording says so.

Upstream, the order those three are asked in is not an implementation detail of a driver. It is
spread across `Prompt.prompt` (one `render()` before any key is read), the tail of `onKeypress`
(render *after* the key has been fully processed, and only then close), and `close()` (write `'\n'`,
then emit `submit`/`cancel`, whose listener writes `cursor.show`). Every one of those decisions is
visible in the bytes. A recorded `text` Fixture is exactly `ESC[?25l`, the whole opening Frame, one
diff per keypress, `\n`, `ESC[?25h` — and the last two are in that order because `close` writes the
newline before it emits, not after.

So the sequence is ported, and it is ported into `clackatui-core` as `Session`, alongside the two
components it orders. A driver supplies keys and a place to put bytes; it does not decide when a
Frame is drawn or what a closing Prompt writes.

## What this buys immediately

A Session performs no I/O, so a Scenario can be replayed through one with no terminal and no
threads: feed it the keys clack was given, concatenate what it returns, and hold the result against
what clack wrote. `tests/scenario_replay.rs` now does that for all ten replayable `text` Scenarios,
and they agree.

The comparison strips SGR from both sides first. The two encode the same appearance differently and
on purpose — clack's Frames arrive as picocolors output, one attribute per escape and each turned
off by name, while the Emitter states a whole `Style` per run and resets (ADR-0011, ADR-0013).
Colour is compared as *styles* by the opening-Frame test that has been there since the widget
landed. What is left after stripping is the part where byte equality is the right question: cursor
movement, erasure, and text — which is to say the diff clack chose for each keypress, and the order
it asked for them in. Flipping the newline and the cursor-show in `close` fails all ten.

This is not the Grid comparison ADR-0001 asks for and does not stand in for one: nothing here knows
where the cursor *ends up*, only which instructions were issued to move it. An emulator is still
what M1 finishes with. But it is the first check in the project that covers a Prompt end to end
rather than one Frame of it.

## Two places the port and upstream part company

**The status after the opening Frame.** Upstream's `state` is one field shared by the state machine
and the writer: `render()` moves it from `initial` to `active` once it has written something. Here
they are separate — the Emitter tracks "have I written a Frame yet" itself and the Prompt's `Status`
stays `Initial` until a keypress moves it. Nothing observable turns on it: clack's `symbol()` draws
`initial` and `active` identically and `TextWidget` matches them in one arm. A later Prompt that
told them apart would need this revisited, which is why it is written down rather than left as a
coincidence that happens to hold.

**Re-wrapping on resize.** Upstream keeps `_prevFrame` as the *wrapped* string and re-wraps it at
the terminal's current width on every render, to count the rows it must walk back over. The Emitter
keeps the rows it last laid out. The two agree whenever the terminal has not narrowed since the
previous Frame — an already-wrapped row cannot wrap again at the same width or a wider one — and can
disagree when it has. No harvested Scenario resizes mid-Prompt, so there is nothing to settle it
against; `Session::resize` is written the straightforward way and the gap is named so the
hand-authored resize Scenario finds it rather than discovers it.

## The one thing in `clackatui` that is a port

Turning crossterm's key events into the ones Node's `readline` would have reported. Everything below
the driver reads a readline `Key` — the Line editor dispatches on `key.name` and `key.ctrl`, and the
Prompt matches aliases against the character, the name and the sequence in that order (ADR-0004) —
so `keys.rs` is not a convenience mapping, it is the last unverified port in the `text` path. Three
of its rules are easy to get wrong:

- a control key still carries its character, and clack's cancel alias is keyed by that string, so a
  decoder that dropped it would leave `ctrl+c` inert;
- punctuation has no name at all, which is what sends it down the Line editor's insertion branch;
- a key decoded from an escape sequence carries no character, because the bytes are not text.

README already lists "key parsing (Node `readline` vs crossterm)" among the Conformance suites and
it is the one still owed a harvest. Until then this is a close reading with hand-written tests, and
it is the least-guarded thing in the project.

## Consequences

- `clackatui-core` gains `session`, and stays free of I/O.
- `clackatui` exists. It is small on purpose: raw mode, a blocking read loop, the key decoder, and
  the `text()` builder. Raw mode is released by a guard rather than at the end of the loop, so a
  panic leaves the user with a working shell; the cursor is restored on the failure path, which is
  the one path upstream does not have.
- `Session` is public, and `Text::session()` hands one over without running it. That is the seam for
  driving a Prompt from an existing event loop — and it is what the tests use, so the thing under
  test is the same object `.interact()` runs.
- What remains of M1 is the emulator: replay the recorded Fixture and the Session's own bytes
  through one `vt100` and compare Grids, cursor included.
