# `select` reads three widths, and a strikethrough outlives its row

The first Prompt with a list in it. Its state is four lines of upstream — a cursor walked by
`findCursor` — and everything else it does is drawing: a message wrapped against one width, an option
list wrapped against another and cut to a height, and a footer whose own height decides how much of
the list survives.

## Two numbers per terminal, one of them new

`Session`'s draw callback now carries a height as well as a width. It has to: upstream's `select`
reads `getColumns(opts.output)` *and* `getRows(opts.output)`, and how many options are drawn is a
function of the second. Nothing else changed shape — the Session already held both numbers, and the
Frame is still wrapped against the width it always was.

The subtraction ADR-0019 records for `confirm` happens twice more here. The message is wrapped to
`columns - 13` and every option to `columns - 13` again, both times because a styled bar and two
spaces are thirteen characters and three columns. So a `select` in an 80-column terminal breaks its
text at 67 and draws it at 3, and the ten columns are lost twice over rather than once.

`select`'s own suite is the first to hand a Prompt a stream narrower than `process.stdout` — 30 and 40
columns against a global 80 — which makes the two widths a Scenario carries visibly different numbers
rather than the same one written down twice. A Scenario that did that *and* resized would be
ambiguous, since a recording says only what the global was resized to;
`the_two_widths_only_come_apart_where_nothing_resizes` is what says no such Scenario exists before one
is written.

## What the Grid caught

Thirteen mutations of the port, all caught; nine of them by upstream's own recordings and four by the
unit tests alone — the empty footer, the unguided continuation bar, and the two cursor skips, none of
which upstream's suite reaches. One of the nine is worth writing down, because nobody would have
predicted it and no amount of reading `select.ts` would have found it:

**A cancelled value's strikethrough leaks onto the Guide bars beside it.** The value is written
`styleText(['strikethrough','dim'], …)` and then wrapped, and `wrapAnsi` reopens the *dim* on every
row while opening the strikethrough once and closing it at the very end. The bars the rows in between
are prefixed with are therefore written inside it, and drawn gray and struck through at the same time.
The port reproduces it (ADR-0013's rule: a terminal can see it), expressed as "the part of the
cancelled style that is not reopened per row" rather than as the strikethrough by name, so a Theme
that changes what a cancelled value looks like changes what leaks out of it too.

Two more are upstream defects reproduced rather than corrected, both around `withGuide: false`. The
bar every message row after the first is prefixed with is passed to `wrapTextWithPrefix`
unconditionally, so an unguided `select` looks unguided until its message wraps. And that bar is
`symbolBar(state)`, not the gray one `confirm` and `text` draw — it is cyan while the Prompt is open
and green once it is submitted.

## `findCursor` is its own module

`utils/cursor.ts` is shared by `select`, `multiselect` and `groupMultiselect` upstream, so it is a
module here rather than a private function of the first Prompt to need it. It takes a predicate rather
than a type, since `disabled` is the only thing it asks of an option, and it is written as a loop
where upstream recurses — the same walk, and no stack to overflow on a long list.

Its wrap is not modular, which reads as a bug until it is read beside the guard above it: a cursor
below zero lands on the last option and one past the end lands on the first, whatever the step was. A
list with nothing selectable in it does not move at all, which is also what makes the loop terminate.

## Consequences

- `Draw` is `Fn(&Prompt<S>, u16, u16) -> Frame`. Every builder and every Scenario closure took the
  extra argument; only `select` reads it.
- `Styles` grows `hint`, `option_disabled` and `instruction_key`. The first two are upstream's `dim`
  and `gray`; the third is the dim on the key in `↑/↓ to navigate`, which is not the same role as a
  hint even though it is the same escape today.
- A `SelectOption`'s label is a `String` from the moment it is built. Upstream computes
  `option.label ?? String(option.value)` at draw time and requires a label for anything that is not a
  primitive; `SelectOption::new` needs `Display` and `SelectOption::labelled` needs nothing, which is
  the same rule enforced a step earlier.
- Still owed: a hand-authored `select` Scenario that resizes. Upstream's suite never resizes anything,
  and the widget's rewrap under one is the same code path `confirm` already has — but that is an
  argument, not a recording.
