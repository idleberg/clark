# `date` is three strings, and a calendar that refuses them

The first Prompt in the port that is neither a text field nor a list. `date` draws a year, a month
and a day side by side, moves a highlight between them with the arrows, and edits the one under the
highlight. It is untracked — `super(opts, false)` — so the vim aliases reach it and readline's line
is never read back: everything it shows, it computed.

## A segment holds a string, and the calendar holds a date, and they disagree

`#segmentValues` is `{ year: '2025', month: '01', day: '__' }`. An underscore is a digit not yet
typed. The three strings are the whole editing state, and the `Date` the Prompt answers with is only
what they happen to mean — recomputed on every keystroke and allowed to be nothing.

Which is why so much of `date.rs` is string arithmetic. `Parts` is what the user is editing; `Date`
is a struct of three numbers, because every question asked of one is a question about the calendar
and none is a question about time. No date crate: `Date.UTC`, `getUTCFullYear` and
`new Date(y, m, 0).getDate()` are the whole of upstream's calendar, and they are forty lines.

## Four things reproduced rather than corrected

Per ADR-0013, wherever a terminal can see it.

- **A year below 100 can never be a date.** `validParts` builds `Date.UTC(year, month - 1, day)` and
  checks the three fields come back unchanged — which catches a day that overran its month, the
  point of the check, and also every year from 1 to 99, which `Date.UTC` silently reads as 1900 + it.
  So `0001` is drawn, accepted by every segment check, and still resolves to nothing; the Prompt
  then demands an answer it is already showing. The authored Scenario
  `date › the arrows fill a blank field with a date that is not one` reaches it in three keypresses,
  because a blank year's arrow minimum is 1.
- **`defaultValue` is documented as a fallback and implemented as a seed.** The constructor reads
  `opts.initialValue ?? opts.defaultValue`, so it is typed into the field on the way in. The
  fallback it also is can only be reached by erasing the field first.
- **A settled Frame draws the segments, not the value.** `render` asks whether `this.value` is a
  `Date` and then prints `formattedValue`. Do both — erase a `defaultValue` and submit — and clack
  answers `2025-12-25` while writing `__/__/____` on the terminal. That is one authored Scenario,
  `date › a default value is answered with while the field shows underscores`, and it is the only
  place in the port where the answer and the drawing of it are different facts.
- **A year can be typed past its own length.** `(digits + char).padStart(4, '_')` pads and does not
  truncate, so a fifth digit into a full year makes it five characters wide. Only a year, and only
  when it is the last segment — one that is not hands the cursor on the moment it fills.

## Two more that are not defects so much as dead ends

- **`invalidDay` is unreachable.** A first digit into a blank day always becomes `0d`, and a second
  only follows a 0, a 1 or a 2. No sequence of digits reaches a day above 29, so the message
  `There are only 31 days in any month` cannot be shown — and 30 and 31 cannot be *typed*, only
  arrowed to. A unit test enumerates all hundred pairs rather than asserting the reading.
- **The completion clamp cannot change anything.** After a segment fills, upstream re-clamps year,
  month and day to their ranges — but only when `validParts` succeeded, which already means they are
  in range. The block is written down here because it is written down there, and a mutation that
  removes it survives; it is marked as such beside the code.

And one behaviour that is neither: **a refused segment traps the next digit.** A rejected two-digit
entry leaves the segment unselected, so the next digit is written into position 0 of what is still
there — a month of `01` that refused a `9` refuses a `7` too, as `71`. Nothing but a backspace or a
move gets out of it. Recorded, because it is four Frames deep and no reading would be believed.

## The locale could not come across

`opts.locale` asks `Intl.DateTimeFormat` which segments to draw, in what order, and with what
between them. Rust's standard library has no locale data, and an ICU crate to answer one question
about three characters is not a trade this crate makes. `DateState::new` takes the order and the
separator outright; `clark::date()` defaults to `MDY` with `/`, which is what `en-US` — the only
locale upstream's suite asks for — resolves to.

The Scenario loader refuses rather than guesses: a Fixture carrying a locale it has no segment order
for panics in the loader, not in parity, because a wrong segment order is not a wrong drawing, it is
a different Prompt. Every authored `date` case passes an explicit `format` for the same reason —
without one the recording would be of the machine that ran the harvest.

`settings.date.monthNames` is not ported at all. It is in upstream's settings, `updateSettings`
merges it, and nothing in upstream reads it: the one place a month name would go says `'any month'`
in a string literal.

## Consequences

- **The thinnest harvest in the port, and the largest authored share.** `date.test.ts` is eight
  Scenarios and nine keypresses, seven of them a bare `return` on a field `initialValue` had already
  filled. It types no digit, presses no arrow, sends no tab and no backspace — so the segment
  editor, which is three hundred of `DatePrompt`'s four hundred lines, reaches no recording. Ten
  authored cases are that editor, and they are the first authored Scenarios written for behaviour
  rather than for a width.
- **The Recorder learned to write down a `Date`.** Four of `date`'s options hold one, and `plain()`
  turned every non-plain object into `{ opaque: true }` — so the first harvest would have recorded
  eight Scenarios with no initial value, no minimum and no maximum, and the port would have agreed
  with all of them for the wrong reason. Both Recorders now write `{ date: <ISO instant> }`.
- **`Theme` gained one style and `settings` one struct.** `date_separator` is `gray`, where the
  `separator` already there is `dim` — the two are written a few lines apart in two files and simply
  disagree. `DateMessages` is upstream's `settings.date.messages`, and it is not a field of
  `Settings`: only one Prompt reads them and it reads them from inside its own state. Two of
  upstream's five are functions of a date, so they keep a `{date}` where the ISO form goes; the third
  is a function of `(days, month)` that is only ever called with `(31, 'any month')`, so it is
  flattened to the string that produces.
- **`PromptState` gained nothing.** `#refresh` calls `_setUserInput`, but with `trackValue` off the
  write into readline is skipped and no branch of `render` asks for the result, so the Prompt's
  `user_input` stays empty here and nothing notices.
- **Still owed, and now for seven Prompts:** a hand-authored Scenario that resizes a list.
