# A task log erases rows it measured with the wrong ruler

M5's sixth unit: `task-log`. It is the third renderer driven by calls rather than by keys, and the
first with nothing turning underneath it — a task log draws only when it is spoken to. What puts it
in the same corpus as the [`spinner`](./0032-a-spinner-walks-the-cursor-back-over-a-string-it-never-wrote.md)
and the [`progress`](./0033-a-progress-bar-is-a-spinner-whose-message-is-drawn.md) bar is the other
half of what those two do: it erases what it wrote before it writes again, and it decides how much
to erase by counting rather than by remembering.

[`clackatui_core::task_log`](../../clackatui-core/src/task_log.rs) is the port.

## The count is a string length

`Math.ceil((line.length + barSize) / columns)`, summed over every line held, is how many rows a task
log believes it put on the terminal. `barSize` is three, for the `│  ` in front of each one. And
`line.length` is a count of **UTF-16 code units**, which is not the number of columns anything took:

- a wide character is one unit and two columns, so eight of them at twenty columns are counted as one
  row and drawn as two;
- an astral character is two units and two columns, so it happens to come out right;
- a message with an escape sequence still in it counts every character of the escape.

Reproduced, escapes and all ([ADR-0013](./0013-the-emitter-diffs-lines-because-clack-does.md)). The
erase is one of the few things in clack that leaves a terminal visibly wrong, and it is wrong here
for messages nobody would call unusual. Six cases in the corpus are that arithmetic at twenty
columns: exactly full, one over, far over, wide, emoji, and once more at an ending.

## The erase is written when nothing was drawn

`message()` clears, appends, and then prints **only if the output is a TTY**. The clearing is not
behind that check. So a CI run — which prints nothing between the title and the ending — still
writes `erase.lines(n)` before every message, and the second message of the run erases rows that
belong to the title. Upstream's own snapshots record it. So does the corpus.

## `withGuide` is taken and never read

`taskLog` accepts a `withGuide` — `CommonOptions` gives it one — and passes it to none of the
`log.message` calls it makes. What those calls read is the global `settings.withGuide`. The option
is therefore dead, and the corpus records both halves: a script that sets it and keeps every bar, and
scripts that turn the global off and lose them. `Options::with_guide` in the port is the global, and
the driver's `.with_guide()` is the knob that works.

The title is not affected either way: those three writes do not go through `log.message` at all.

## An empty row keeps its two spaces

Everything a task log prints goes through `log.message`'s `string[]` overload, with every row already
styled — `styleText('dim', line)`. That overload drops a row's prefix when the row is empty, and a
dim-styled empty string is not empty: it is four escape characters. So a blank line inside a task log
is `│  ` where a blank line inside a plain `log` is `│`.

Saying that without escapes is what [`crate::message::log_lines`](../../clackatui-core/src/message.rs)
is: it takes drawn `Line`s, and a row counts as blank only when it has no spans at all. `log`'s own
string path maps an empty line to `Line::blank()`; a task log's maps it to one empty styled span. Two
callers, one predicate, and the difference upstream makes by accident is made on purpose.

## The one divergence

`stripDestructiveANSI` removes the escapes that move or erase and leaves the rest — so a caller who
puts colour in a message gets colour. A Frame carries no escapes
([ADR-0011](./0011-a-frame-carries-a-style-per-span-and-no-escapes.md)) and the renderer drops any it
is handed, so here that colour reaches the terminal as nothing at all.

This is the first place in the port where clack can be handed something this crate cannot say, and it
is not being fixed: the way to colour a task log's row is a `Line`, which is what `log_lines` already
takes. There is no case in the corpus for it, because a recording of it would be a recording of a
disagreement. There is a unit test, so that it is a decision rather than a surprise.

## Consequences

- **The corpus grew a third kind.** `scripts/scripted/` now carries **114 scripts and 735 steps** —
  forty-two spinners, twenty-four bars, forty-eight task logs. A task log's script opens (the title is
  written from the constructor, so making one is a step), says things, and ends. Its ops are
  `open`, `message`, `group`, `group-message`, `group-success`, `group-error`, `success`, `error`.
- **The Recorder learned two globals.** `output.isTTY`, because half of what a task log calls `isTTY`
  is a property rather than an environment variable; and `settings.withGuide`, restored after every
  case, because it is the only way to record the Guide going off.
- **`erase.lines(n)` is now in the Emitter**, where `erase.lines(1)` already was under the name
  `erase_row`. One function, two callers, and the step-up written between the erases rather than
  after the last one — which is what makes the cursor end on the topmost row.
- **The driver has no thread.** `progress` reused the spinner's interval; a task log needs no
  interval, so `clackatui/src/task_log.rs` is a builder, a struct and eleven methods that print.
- **Forty-seven mutants, forty-five caught.** Three of the first run's five survivors were gaps and
  are now cases: a blank row on a terminal **two columns wide**, which is the only width where
  `line === '' ? 1` is not the answer the arithmetic already gives; a group whose ending is taller
  than the rows it replaced, which is the only shape where counting the result rather than the rows
  is visible; and a run that carries on after an ending, which is the only way to see that the groups
  went with it.
- **The last two survivors are each other's.** `retainLog` is checked twice — once where the dropped
  rows are kept and once where they are printed — and either check alone does the whole job, so
  removing one changes nothing a terminal can see. Both are upstream's, and both stay.
