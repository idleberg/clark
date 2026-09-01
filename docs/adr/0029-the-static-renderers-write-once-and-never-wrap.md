# The static renderers write once and never wrap

M5's first unit: `log`, `intro`, `outro` and `cancel`. None of them is a Prompt. There is no state
machine, no keypress, no second Frame and no cursor to hide — a caller says something, one string
goes out, and the interaction is over. That is a smaller thing than anything else in the port, and
it needed two decisions and a third corpus.

## `write_once`, beside the Emitter and not inside it

`Emitter::frame` is a port of `Prompt.render`: it holds the previous Frame, diffs it row by row, and
derives every cursor movement from row counts (ADR-0013). A static renderer reaches none of that.
Upstream builds a template string and hands it to `output.write`.

So `emitter::write_once` is a free function beside the Emitter rather than a method on it: the rows
of one Frame, joined with `\n`, and one `\n` after the last. No hidden cursor, no diff, no previous
Frame to be wrong about. Putting it in the same module is the whole of the reuse — `write_rows` and
`Frame::rows` are what turn Styles into SGR, and there should be exactly one of those.

**Nothing here wraps, and that is the load-bearing part.** clack wraps a Prompt's Frame itself
because it has to count the rows it walks the cursor back over (ADR-0012); nothing walks back over
these, so a long line goes out whole and the *terminal* breaks it. `write_once` therefore lays the
Frame out at `u16::MAX` and lets the emulator do what a terminal would. Three cases in the corpus
are longer than the terminal they are written to, and they are what would notice if that changed.

## The trailing blank row is a real row

Every one of these ends its write with `\n`, and `outro` and `cancel` end with two. A Frame is a
list of rows and carries no trailing anything, so the second `\n` is written down as a blank `Line`
at the end of the Frame. `intro` has none, which is why an `intro` and an `outro` sit at different
distances from what follows them.

## A third corpus, and the weakest oracle of the three

`scripts/static/` records `scripts/static/cases.mjs` against clack the way `scripts/authored/` does,
minus everything a Scenario needs. There are no keypresses to deliver in order and no `value` to
declare: a static renderer is a function from a message to a string, so a case that ran at all
recorded the whole of what clack does, and the only precondition worth asserting is that it wrote
*something*.

Thirty cases. Upstream's own `log.test.ts` is good — it is where the empty-row branch, the custom
symbols and the `spacing` option come from — and `intro`, `outro` and `cancel` have no test file at
all. What no upstream test covers anywhere is a message wider than the terminal, which is the one
claim these renderers make that a Prompt's Frame never does.

## Two comparisons, because a Grid cannot see the end of a row

`static_parity.rs` compares the Grid, as ADR-0001 requires, and then compares the characters with
SGR stripped. The second is not redundant here. `outro('')` writes the corner, two spaces and
nothing else; on an 80-column Grid those two spaces are indistinguishable from the blanks the
terminal pads every row with. Trailing space is invisible to an emulator and perfectly visible to
anyone who selects the row, so it gets a comparison of its own — and it is a real upstream
behaviour, kept per ADR-0013 rather than trimmed.

## The emulator harness is now shared

`Grid`, the `ONLCR` translation, the row-by-row `difference` report and the read-once `characters`
moved out of `scenario_parity.rs` into `tests/grid/mod.rs`, which both parity tests include. The
arbiter of every appearance claim in the project should be one thing: a Grid built two slightly
different ways in two files is two claims wearing one name.

## Consequences

- **`Theme` gained six styles.** Five for the `log` symbols and one for `cancel`'s message —
  upstream's plain red, which is not the strikethrough `cancelled` a Prompt's own abandoned value is
  drawn in. `log_info` is blue, and it is the only blue in clack.
- **`log` takes a `&str`, not a `&str` or a `&[&str]`.** Upstream's `string | string[]` splits the
  string on `\n` and does not split the array's elements, so `log.message(['a\nb'])` writes one row
  with a newline inside it and no bar on its second line. The overload has no counterpart here and
  the array half is not ported; the string half is the one every caller and every upstream test
  uses.
- **`log.message()` with no argument at all has no counterpart either.** The default is `[]`, which
  is a message of *no* lines — the only way to reach that branch. From a `&str` the emptiest message
  is `""`, which is one empty row.
- **The `clackatui` side returns nothing and drops a failed write.** These are called for their side
  effect in the middle of a program that has nothing useful to do about a broken pipe, and a
  `Result` on every log line is a `Result` nobody reads. A Prompt is the other way round: the driver
  reports every failure, because a Prompt that cannot draw cannot be answered either.
