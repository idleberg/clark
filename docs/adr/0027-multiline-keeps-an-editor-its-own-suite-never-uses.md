# `multiline` keeps an editor its own suite never uses

The last Prompt in the port, and the only one that keeps a text editor of its own. `text` hands its
keys to readline and reads the line back off `rl.line`; `multiline` is `super(opts, false)` —
untracked — and does every insertion, deletion and cursor move itself, over a string with newlines
in it. `MultiLineState` therefore owns the text and the cursor outright, and `Prompt::user_input` is
not what gets drawn.

## `return` is not submission, and that is why `_shouldSubmit` exists

Every other Prompt in clack submits on `return`. This one inserts a newline and answers *no*; two of
them in a row, with the cursor at the end of the text, answer yes — and take the first newline back
out again on the way, so the value never carries the keystroke that ended it. With `showSubmit` on
the rule changes entirely: `return` is only ever a newline, tab moves the focus to a `[ submit ]`
button, and `return` on the button is what settles the Prompt.

`PromptState::should_submit` has been in `prompt.rs` since M1, unused, with a doc comment naming
this Prompt as the reason. Porting it needed one change: `&self` became `&mut self`. Upstream's
`_shouldSubmit` is not a predicate — it edits the text and moves the cursor in the course of
returning `false` — and it runs *after* the `key` listener and *before* validation, which is exactly
where that has to happen, because the text it edits is the text the validator then sees.

## Tab is matched by its character, not by its name

`if (char === '\t' && this.#showSubmit)`. Every other place in clack that cares about a tab reads
`key.name`. A terminal that reports a tab with no character therefore does not move the focus here,
and the authored recordings had to send `{ s: '\t', name: 'tab' }` where the `date` ones send
`{ s: '', name: 'tab' }` — the two Prompts read different halves of the same keypress. This was
found by two authored cases hanging for five seconds and timing out, which is the recorder doing its
job: a case that declares its value and never reaches it is not written down.

## Three things reproduced rather than corrected

Per ADR-0013, wherever a terminal can see it.

- **The error foot is drawn whether or not there is a Guide.** `errorPrefixEnd` is built and
  interpolated unconditionally, while the `lines` above it are guarded by `hasGuide`. So
  `withGuide: false` on a `multiline` that fails validation still puts a `└` in the left margin with
  nothing above it — alone among the Prompts, all of which guard both.
- **A settled Frame with no value still draws a bar and two trailing spaces.**
  `wrapTextWithPrefix(output, '', prefix)` wraps the empty string, which is one line, and prefixes
  it — so the row is the bar plus two spaces. `text` reaches the same case by a different route and
  draws a bare bar. Nothing else in the port leaves trailing whitespace on a settled row.
- **The text is wrapped thirteen columns early.** `wrapAnsi(text, columns - prefix.length)`, where
  the prefix is a bar wrapped in two escape sequences: `ESC[36m`, `│`, `ESC[39m`, two spaces —
  thirteen characters for three columns. ADR-0019 records it for `confirm`'s message and ADR-0024
  for a `groupMultiselect` option; this is the third place, and the first where it applies to what
  the user typed rather than to what clack wrote.

## One dead end, ported as it stands

`if (this.userInput[this.cursor - 1] === '\n')` guards the take-back, and the guard cannot fail.
`#lastKeyWasReturn` is set by nothing but a `return`; a `return` only ever sets it after inserting a
`\n` at the cursor and stepping past it; and every other key clears it. So by the time the take-back
is reached, the character before the cursor is always the newline it is looking for. Kept because it
is written down there — a mutation that widens it to "remove whatever is there" survives, and the
code says so beside it.

## `findTextCursor` remembers no goal column

The other half of `utils/cursor.ts`, unported until now. A vertical move carries the offset *within*
the current row to the new one and clamps it to that row's length, so walking down onto a short line
and back up does not come home. A horizontal move crosses rows: left from column zero lands at the
end of the row above, because the two `while` loops carry an out-of-range column onto its neighbour.
Both ends absorb.

Upstream's offsets are UTF-16 indices; these are counts of characters. The two agree on everything
but astral characters, where upstream counts two and this counts one — the same trade
`InputWithCursor` makes, and for the same reason: a cursor here always sits between characters,
where upstream's can land inside one.

## Consequences

- **The thinnest editor coverage in the port.** `multi-line.test.ts` sends thirty-eight keypresses,
  of which thirty are a bare `return` and eight are single characters. It never moves the cursor,
  never deletes anything, never starts the field with text in it, and never varies the terminal — so
  the editor and the wrap, which between them are most of both files, reach no recording. Nine
  authored cases are those: the arrows, backspace, delete, an `initialValue` that opens two rows
  tall, the button at a width its field wraps at, and a terminal that narrows under text already
  several rows high.
- **The Scenario loader was measuring against a width that could not move.** Every list Prompt's
  arm captured `stream_columns` at build time, so a widget's idea of the terminal never followed a
  resize. Nothing had noticed because no Scenario both wrapped inside a widget and resized — until
  the ninth authored case here did, and disagreed with clack by two rows. `Scenario::stream_width`
  now decides it once for every arm: the live width, except where a Fixture sets the Prompt's stream
  apart from `process.stdout`, which
  `the_two_widths_only_come_apart_where_nothing_resizes` already guarantees never happens under a
  resize.
- **`Theme` gained two styles and `text` lost three lines.** `submit_focused` and `submit_unfocused`
  are the button's cyan and dim. The placeholder block `text` and `multiline` both write is now
  `text::placeholder_spans`, shared rather than copied.
- **`PromptState` gained nothing but a `mut`.** `should_submit` was already declared; `TRACKS_INPUT`
  was already the switch that keeps readline out of the way.
- **Forty-seven mutations, forty-one caught first time.** Three survivors were real gaps, now closed
  by three tests: a removal that took one byte rather than one character (invisible in ASCII, a panic
  in anything else), the tab that is matched by its character, and `_cursor++` moving one place
  however much a keypress carried. The other three are equivalent and each is written down beside the
  code it could not change — the vertical clamp written as a `max` and a `min`, the `delete` guard
  whose absence removes nothing observable, and the dead take-back guard above.
- **Still owed, and now for seven Prompts:** a hand-authored Scenario that resizes a *list*. This
  one resizes a text field, which is the case that found the loader bug — but a list re-lays-out
  through `limitOptions`, and nothing has put a recording under that.
