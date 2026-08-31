# `multiselect` draws an error a Frame cannot carry

The second list Prompt, and the first whose answer is a set. Almost all of it is `select` again —
the same `Option` type, the same `findCursor`, the same `limitOptions`, the same thirteen columns
subtracted twice — and the port reuses each of those rather than restating them. What is new is
three things: a selection that `space` toggles, a settled value made of several styled pieces, and a
validation failure.

## The validator returns something a Frame has no room for

Upstream's `multiselect` writes its own `validate` and there is no way to pass another. The string it
returns is two lines, and the second has escapes in it:

```
Please select at least one option.
Press ␣ space ␣ to select, ␣ enter ␣ to submit
```

— where each key is `styleText(['gray','bgWhite','inverse'], ' space ')`, and the whole second line
is wrapped in a `reset` inside a `dim`. A `Prompt`'s error is a `String` and a Frame holds styling as
a `Style` per span with no escapes anywhere (ADR-0011), so that line cannot travel the way upstream
sends it. Writing the escapes into the error string would put ANSI back into the one place the
architecture is built on its absence.

So the error is split where the architecture already cuts: the Prompt carries the sentence, the
widget draws the advice. `multi_select::required` is the validator and `REQUIRED_ERROR` is all it
returns; `MultiSelectWidget::error_footer` draws both rows, taking the chip's colours from the Theme
like everything else visible. The Grid is identical, and a Theme can now restyle a message upstream
hard-codes.

This works because a `multiselect` has exactly one error. A Prompt that composed a user's validator
with its own would need the two halves to travel together, and that is a problem M4's date prompts
may bring back.

## A leak is a property of a break, not of a value

ADR-0021 recorded that `wrapAnsi` reopens `dim` on every wrapped row and opens `strikethrough` once,
so the Guide bars between the rows of a cancelled `select` are drawn struck through. A `multiselect`
settles on *several* labels joined by a dim `, `, and each label carries its own `9m`…`29m` pair. The
leak therefore depends on where the break landed: inside a label and the strikethrough is still open
across the next row's bar; at a separator — on either side of it — and it is not, because the label's
own escape was closed there.

What survives a break is what the text on *both* sides of it carries, which is an intersection, and
`carried` computes it. The `leaked` helper itself moved to `wrap`, where the `wrapAnsi` behaviour it
describes is already documented and where `select` and `multiselect` can both reach it.

The settled value is built as one `Line` of spans and wrapped as a unit, because upstream builds one
styled string and hands it to `wrapTextWithPrefix` — the labels and the separators break as one piece
of text rather than one at a time, and `Line::wrap` is that said structurally.

## A third unguided bar

`select` has two bars a `withGuide: false` does not switch off (ADR-0021). `multiselect` has a third
and it is the strangest: a cancelled `multiselect` whose selection is empty returns
`` `${title}${styleText('gray', S_BAR)}` `` before it consults `hasGuide` at all. So an unguided
`multiselect` that is cancelled with nothing ticked draws a Guide bar and nothing else. Reproduced,
on ADR-0013's rule.

The emptiness is tested on the *joined labels* rather than on the selection, so one option labelled
with spaces settles the same way none at all does. The port asks the same question of the same
string.

## Consequences

- `Styles` grows seven: three checkbox states, a ticked option's label, a disabled option's label,
  and the two the error advice needs. The checkbox needs three where a radio needed two because it
  answers two questions at once — is the cursor here, and is this one chosen — and upstream gives
  each reachable answer a colour.
- `Color::Gray`, not `Color::White`, is Node's `bgWhite`. SGR 47 against SGR 107; the Theme's table
  now says so.
- A `multiselect`'s answer is in the order the boxes were ticked, not the order the list draws them.
  That is upstream's `[...this.value, this._value]`, and it is invisible from a recording because
  every Frame filters the option list before drawing it. It is visible from `interact()`, so it is
  pinned by a unit test.
- `toggleAll` counts rather than compares, and the port counts too. A `multiselect` started with an
  `initialValues` holding a disabled option's value can reach the count without holding every enabled
  option, and the first `a` empties the list instead of filling it.
- Still owed, and now for two Prompts: a hand-authored Scenario that resizes a list. Upstream's
  suites never resize anything.
