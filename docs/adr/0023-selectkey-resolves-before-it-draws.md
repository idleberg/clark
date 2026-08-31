# `selectKey` resolves before it draws

The third list Prompt and the smallest. There is no `limitOptions`, no instruction footer, no
validator and no cursor to move: the whole list is drawn, the value's first character chooses it, and
pressing that character ends the Prompt. Almost all of what is left is `select` again — the same
`Option` type, the same thirteen columns subtracted for a three-column prefix — and the port reuses
both. What is new is one ordering, one Theme entry, and a correction to the harness that has been
comparing Grids since M1.

## The promise resolves from inside the `key` listener

`SelectKeyPrompt` sets `this.state = 'submit'` and calls `this.emit('submit')` from inside its own
`key` handler. The `once('submit')` registered by `prompt()` writes `cursor.show` and resolves —
and then `onKeypress` carries on, renders the settled Frame, and calls `close()`, which finds the
listener already gone and writes its newline alone. So the cursor comes back *before* the settled
Frame instead of after the newline that follows it.

`confirm` does something like this and it is not the same thing (ADR-0018): its listener calls
`close()` itself, so three writes move rather than one. Two mechanisms, then, and they are kept
apart: [`Prompt::closed_early`] is `confirm`'s and `Prompt::resolved_early` is this one's, set by
`PromptState::submits_from_key` after the state has seen the key. A `Session` already tracked
whether the `once` listener had run — it has to, because a Prompt that closes twice shows the cursor
once — so the new path is four lines that set that flag early.

## `bgCyan`, and the difference between two whites

The chip beside an option is `styleText(['bgCyan','gray'], ' a ')` where the cursor opened and
`styleText(['gray','bgWhite','inverse'], ' b ')` everywhere else. The second is the same three codes
as `multiselect`'s error chip without the `dim` that wraps its advice, so the Theme grows two rather
than reusing one: `key_active` and `key_inactive`. `Color::Gray` is Node's `bgWhite` (SGR 47) and
`Color::White` is SGR 107 — a distinction ADR-0022 already had to make, and one the Grid catches.

## Three defects reproduced

- **A cancelled `selectKey` always draws the first option.** The `cancel` branch reads
  `this.options[0]` and never `this.value`, so cancelling after opening on the third option shows
  the first.
- **A submitted one falls back to it too.** `find(o => o.value === this.value) ?? opts.options[0]`
  against a value nothing set — which is what a bare `return` submits, since only a matching key
  ever assigns one — draws the first option under a green tick, having answered `undefined`.
- **`initialValue` is compared with the initials, not the values.** Upstream builds
  `keys = options.map(({ value: [initial] }) => …)` and then asks `keys.indexOf(opts.initialValue)`,
  so only a one-character `initialValue` can ever match; case-insensitively the initials are folded
  and the `initialValue` is not, so an uppercase one cannot match at all.

The message is also the one list Prompt's that is never wrapped — it is interpolated into the title
and left for the Frame's own wrapping — so a `selectKey` whose message breaks has a second row with
no prefix, where `select`'s has a bar.

## The emulator was reading cells no terminal has

`selectKey` is the first Prompt whose suite cancels on a value long enough to wrap, and it failed the
Grid comparison on one cell: the Guide bar of the last row of a cancelled value was struck through by
clack and not by the port. The port was right and so was the recording. The harness was wrong.

`avt` is an emulator, not a line discipline, and it had been fed `\n` as a bare line feed. clack
writes its Frames to `process.stdout` — it puts the *input* into raw mode and leaves the output
alone — so every newline in a Frame is a carriage-return line feed by the time a terminal sees it.
Without that translation every Frame was drawn down a staircase, each row starting where the last one
ended, and the cells to the left of each row were ones nothing ever wrote. `avt` fills them with
whatever style was open at the time, so the comparison was reading a difference in the escape *before*
a newline off cells that do not exist. `scenario_parity.rs` now translates, and the staircase and the
phantom cells are gone.

That is also the answer to a question the Emitter's port left open: `ESC[999D` is written before
every cursor walk because a Frame whose last row has no newline after it leaves the cursor at the end
of that row, not because clack expects a staircase.

## Consequences

- `Styles` grows two, and `theme.rs`'s SGR table grows a row for `bgCyan`.
- `PromptState` grows `submits_from_key`, defaulting to false. It is the second of two hooks for a
  Prompt that settles inside a listener, and the ADR either of them points at is the one that says
  why there are two.
- `Line::paragraphs` moved out of `multi_select` into `frame`, where `Line::wrap` already lives.
  Both list Prompts hand a wrapper one paragraph at a time, because `wrapAnsi` breaks on `\n` before
  it breaks on width and styles the pieces separately.
- The `select-key` Fixture's Scenarios are all named `text › …`, because upstream's
  `select-key.test.ts` opens `describe.each([...])('text (isCI = %s)')` — a copy-paste from
  `text.test.ts`. Two of the names collide with the `text` Fixture's own. Left alone: a Fixture is a
  recording, and renaming a case to make a failure message read better is editing the evidence.
- Still owed, and now for three Prompts: a hand-authored Scenario that resizes a list.
