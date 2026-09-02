# clackatui

A Rust adaptation of [clack](https://github.com/bombshell-dev/clack), built on
[Ratatui](https://ratatui.rs), whose appearance is verified against the JavaScript original rather
than merely modelled on it.

Status: **M0, M1 and M2 done.** The `ForcedWidth` probe passed, so the architecture below holds —
with one correction recorded in
[ADR-0007](./docs/adr/0007-forced-width-holds-but-the-emitter-owns-shrink-repaints.md). Two of M1's
ported primitives have landed, each against a harvested oracle
([ADR-0008](./docs/adr/0008-width-parity-is-asserted-against-a-harvested-fixture.md)): the width
port agrees with `fast-string-width` on all 82 cases of its corpus, and the `LineEditor` agrees with
Node's `readline` on all 493 keypresses of its own. On top of them sits the Prompt state machine and
`TextState`, ported from `@clack/core` with its event emitter replaced by a trait
([ADR-0009](./docs/adr/0009-a-prompt-owns-its-state-instead-of-subclassing-it.md)). The Recorder
that gives them their oracle now runs
([ADR-0010](./docs/adr/0010-the-recorder-instruments-clacks-suite-from-outside-it.md)): clack's own
`text` suite is harvested into 13 Scenarios, and replaying their keypresses settles where clack
settled. Frames now draw: a `Frame` is styled text with no escapes in it, and its `Widget` places
one width segment per cell under `ForcedWidth`
([ADR-0011](./docs/adr/0011-a-cell-holds-one-width-segment.md)). On top of it sit the Theme, ported
from clack's `common.ts`, and the `text` widget — and the opening Frame each Scenario records is a
Frame clack wrote whole rather than as a diff, so all 13 are compared against it directly, colours
and all. Rows are clack's too: `fast-wrap-ansi` is ported and agrees with the original on all 47
cases of its corpus, because clack wraps its Frames itself rather than letting the terminal do it
([ADR-0012](./docs/adr/0012-clack-wraps-its-own-frames.md)). The Emitter that turns Frames into
bytes is ported too, and agrees with `@clack/core`'s own `render` byte for byte on all 40 cases of
its corpus — it diffs lines rather than cells, because that is what upstream does and the difference
is visible in where the cursor ends up
([ADR-0013](./docs/adr/0013-the-emitter-diffs-lines-because-clack-does.md)). And `text` now runs:
`clackatui::text("What is your name?").interact()`. The order a Prompt is asked in — one Frame
before any key, one after each, a newline and only then the cursor — is visible in every recording,
so it is ported into the core as a `Session` rather than left to a driver
([ADR-0014](./docs/adr/0014-the-sequence-is-a-compatibility-surface.md)); with it, all ten
replayable `text` Scenarios are written the way clack wrote them, styling aside. And now both byte
streams go through one emulator, and all ten agreed on the Grid — characters, styles and cursor
position. That is the comparison [ADR-0001](./docs/adr/0001-parity-is-measured-on-the-grid.md) is
built around and the thing M1 existed to reach. One requirement in it cannot be met — no emulator
models conceal, which is half of how clack draws its text cursor — so the Grid runs beside the
Frame-level style comparison rather than replacing it
([ADR-0015](./docs/adr/0015-the-emulator-is-avt-and-it-cannot-see-conceal.md)). Upstream's tests
never vary the terminal, so eleven more Scenarios are hand-authored against the same clack — 40 and
20 columns, CJK text, a wrap that grows as a value is typed and shrinks again as it is deleted, and
four that resize the terminal under an open Prompt — and **all twenty-one agree**. Recording them
turned up the width clack really wraps to, which is not the one the Recorder had been writing down
([ADR-0016](./docs/adr/0016-hand-authored-scenarios-and-the-width-clack-actually-wraps-to.md)), and
the resizes settled the last divergence
[ADR-0014](./docs/adr/0014-the-sequence-is-a-compatibility-surface.md) had left open: two of them
disagreed, upstream turned out to walk the cursor back over rows it never drew, and the port now
does the same
([ADR-0017](./docs/adr/0017-restore-cursor-walks-back-over-rows-that-are-not-there.md)).
**M1 is done.** M2 adds `password` and `confirm`, and cost what M1's rationale said it should: two
widgets, two states, two builders, and nothing structural. Everything else was already
Prompt-agnostic, so harvesting clack's own `password` and `confirm` suites was two commands and
twenty-one more Scenarios. Both Prompts turned out to do something no `text` does, and all three
findings are things a terminal can see: a `confirm` settles from inside its own listener and writes
four sequences in an order no driver would arrange, and a `password` clears its own field from
inside the callback that draws it
([ADR-0018](./docs/adr/0018-a-render-callback-that-writes-and-a-listener-that-closes.md)); and a
`confirm` wraps its message against the *length of the escape sequence* that styles its Guide, so it
breaks ten columns early
([ADR-0019](./docs/adr/0019-confirm-wraps-its-message-against-the-length-of-an-escape-sequence.md)).
None is reachable from upstream's own tests, so eleven more Scenarios were hand-authored to reach
them, and all forty-nine agree on the Grid. M3 adds the first Prompt with a list in it: `limitOptions`
ported against a corpus of its own
([ADR-0020](./docs/adr/0020-limit-options-is-ported-against-a-corpus-and-reaches-a-width-of-nothing.md)),
then `select` on top of it, whose own suite is the first to hand a Prompt a terminal narrower than the
one its Frames are written into
([ADR-0021](./docs/adr/0021-select-reads-three-widths-and-a-strikethrough-outlives-its-row.md)), then
`multiselect`, whose validator returns a message with escapes in it that a Frame has no way to carry
([ADR-0022](./docs/adr/0022-multiselect-draws-an-error-a-frame-cannot-carry.md)), then `selectKey`,
which resolves its promise from inside its own key listener and whose suite caught the emulator
reading cells no terminal has
([ADR-0023](./docs/adr/0023-selectkey-resolves-before-it-draws.md)). M4 opens with
`groupMultiselect`, which wraps each option against a prefix measured with its escapes still in it
and then wraps the result again
([ADR-0024](./docs/adr/0024-groupmultiselect-measures-a-prefix-nobody-can-see.md)), and then
`autocomplete` and `autocompleteMultiselect`, two Prompts out of one file whose search box draws its
caret at the *option list's* index
([ADR-0025](./docs/adr/0025-autocomplete-slices-the-search-box-at-the-option-cursor.md)), and then
`date`, three strings edited beside a calendar that refuses a third of what they can spell
([ADR-0026](./docs/adr/0026-date-is-three-strings-and-a-calendar-that-refuses-them.md)), and closes
with `multiline`, which keeps an editor of its own and whose `return` is a newline rather than an
answer
([ADR-0027](./docs/adr/0027-multiline-keeps-an-editor-its-own-suite-never-uses.md)) —
**all a hundred and ninety-one agree on the Grid**.
M5 leaves the Prompts behind for the output that is not one: `log`, `intro`, `outro` and `cancel`
write once, never wrap, and are recorded by a third harvest of their own — the first thing in the
port that hands a line to the terminal and lets *it* do the breaking
([ADR-0029](./docs/adr/0029-the-static-renderers-write-once-and-never-wrap.md)). `note` is the
exception that proves it: it draws a right-hand border, so it has to measure first, and its
formatter returns a drawn `Line` where upstream's returns a string with escapes in it
([ADR-0030](./docs/adr/0030-a-notes-formatter-returns-a-line.md)). `box` is `note` the other way
round — it settles on a width first and makes the content fit — and porting it against a corpus
found two of its options doing the opposite of what they document
([ADR-0031](./docs/adr/0031-a-boxs-defaults-are-not-the-documented-ones.md)) — **eighty-seven static
cases, all agreeing on the Grid and character for character**. `spinner` is the first renderer that
draws more than once and the first with a clock in it, so it needed a fourth Recorder whose cases are
*scripts* — `start`, some ticks of a fake clock, a `stop` — and it walks the cursor back over a
string three columns narrower than the one it drew
([ADR-0032](./docs/adr/0032-a-spinner-walks-the-cursor-back-over-a-string-it-never-wrote.md)).
`progress` is that same spinner with a drawn bar for a message, which is the whole of the port
([ADR-0033](./docs/adr/0033-a-progress-bar-is-a-spinner-whose-message-is-drawn.md)). `task-log` is
the third of them and the first with no clock at all: it erases what it wrote by counting UTF-16 code
units, which is not what the terminal counted, and it writes that erase even in CI where it printed
nothing
([ADR-0034](./docs/adr/0034-a-task-log-erases-rows-it-measured-with-the-wrong-ruler.md)) — **a
hundred and fourteen scripts and seven hundred and thirty-five steps, every step byte for byte**.
`path` closes M5 and is the only module in clack that reads something outside the terminal: an
`autocomplete` whose options are a *function* and therefore have no filter at all, so what narrows
the list is `readdirSync` and a prefix test. Its `Fs` is the seam the architecture has owed since
M0, and because upstream's suite mocks `node:fs` with `memfs`, the Recorder now writes the volume
down beside the keypresses and the Scenarios replay against the same one
([ADR-0035](./docs/adr/0035-a-path-is-an-autocomplete-whose-filter-is-a-filesystem.md)) — **thirteen
more Scenarios, all agreeing on the Grid**. **M5 is done.**
See
[CONTEXT.md](./CONTEXT.md) for the vocabulary and [docs/adr/](./docs/adr/) for the decisions behind
the shape below.

## Compatibility target

| | |
|---|---|
| clack | `@clack/prompts@1.7.0`, `@clack/core@1.4.3` (published; pinned by lockfile) |
| Ratatui | `ratatui-core` 0.1.2 (pinned exactly — pre-1.0, expect churn) |
| Terminal I/O | crossterm 0.29 |
| Emulator | `avt` 0.18 — the arbiter of every appearance claim ([ADR-0015](./docs/adr/0015-the-emulator-is-avt-and-it-cannot-see-conceal.md)) |

## Shape

Two crates:

- **`clackatui-core`** — state machines and `Widget` impls over `ratatui-core`. No I/O. Feed it a
  key event, render it into a `Buffer`.
- **`clackatui`** — a blocking driver plus clack's sugar (intro, outro, log, note, box, spinner,
  progress, task log, group). No async runtime imposed. Small on purpose: raw mode, a read loop, the
  crossterm-to-`readline` key decoder, the spinner's interval, and the builders. The order a Prompt is asked in is in the core, not here
  ([ADR-0014](./docs/adr/0014-the-sequence-is-a-compatibility-surface.md)).

Ratatui is used as a primitives library, not as a framework: `Buffer`, `Cell`, `Style` and the
`Widget` traits, chiefly for `BufferDiff`. `Terminal` is not used
([ADR-0002](./docs/adr/0002-own-inline-emitter.md)), and neither is `Layout` nor `ratatui-widgets`
([ADR-0006](./docs/adr/0006-depend-on-ratatui-core-for-primitives-only.md)). Cells are placed
individually with `CellDiffOption::ForcedWidth`; the `Buffer::set_string*` family is never called,
because it measures under a width model we reject
([ADR-0005](./docs/adr/0005-port-fast-string-width-not-unicode-width.md)).

```rust
let name = text("What is your name?").interact()?;        // Result<T, ClackError>

match confirm("Continue?").interact_opt()? {              // Result<Option<T>, ClackError>
    Some(v) => …,
    None => /* cancelled — carry on */,
}
```

`.interact()` bubbles a cancel as `ClackError::Cancelled`, which is what most CLIs want.
`.interact_opt()` gives clack's cancel-as-value semantics, and is what `group()` is built on —
clack's `group()` writes `'canceled'` into its results, calls `onCancel`, and continues to the next
Prompt, which an unwinding cancel could not express.

## Scope

**v1** — the 12 interactive Prompts (text, password, confirm, select, multi-select, select-key,
group-multi-select, autocomplete, date, multi-line, path) and the static renderers (log, note, box,
intro, outro, cancel, spinner, progress-bar, task-log, group, limit-options).

**v2** — task, stream.

The rest of that set is deferred because it is non-deterministic, not because it is unimportant:
`spinner.ts` is the only clack module touching timers, `task` is built on it, and `path.ts` reads the
real filesystem. The spinner turned out to need no `Clock` abstraction at all — `clackatui-core`
takes the elapsed time as an argument and the driver owns the interval, and the corpus that records
it drives a fake clock through a script of calls
([ADR-0032](./docs/adr/0032-a-spinner-walks-the-cursor-back-over-a-string-it-never-wrote.md)), which
is why `progress-bar` moved up rather than waiting for one
([ADR-0033](./docs/adr/0033-a-progress-bar-is-a-spinner-whose-message-is-drawn.md)), and `task-log`
after it — it has no clock, only the same counted erase
([ADR-0034](./docs/adr/0034-a-task-log-erases-rows-it-measured-with-the-wrong-ruler.md)). `Fs` is
the one seam that was really owed, and `path` pays it: three questions asked of a filesystem, with
`std::fs` answering them in the driver and the volume a recording carries answering them in the
tests ([ADR-0035](./docs/adr/0035-a-path-is-an-autocomplete-whose-filter-is-a-filesystem.md)) — so
what is left deferred is `task` and `stream`, neither of which is a Prompt. `Theme` is the same kind
of seam, and its `Default` is `Theme::clack()`,
ported from clack's `common.ts`. `is-unicode-supported` is ported with it, as `Theme::detect()`.
`FORCE_COLOR` is not: upstream suppresses colour by having `styleText` return the string unchanged,
and a Frame here carries a `Style` rather than escapes, so the equivalent seam is where Styles become
bytes — the Emitter's, not the Theme's.

One scope note worth recording: clack runs its prompt tests twice, under `CI=true` and `CI=false`,
but `CI` only affects `spinner.ts` and `task-log.ts`. Every other Prompt needs one pass, not two —
and a spinner's second pass is not a second run of the suite here but cases of its own, `CI` being an
option on the port rather than an environment it reads.

## Testing

Three layers.

1. **Prompt Scenarios** — harvested from clack's own test suite, plus hand-authored coverage of what
   upstream never varies: narrow terminals, CJK input, long values, a terminal that changes size
   under an open Prompt. `cargo test` replays both the recorded Fixture and the port's own bytes
   through one emulator (`avt`) and compares Grids — characters, styles and cursor position, with
   the emulator resized at the same points in both streams. Two Recorders write the Fixtures, both refusing unless
   the clack checkout is at the pinned tag: `node scripts/harvest-scenarios.mjs <prompt>` runs
   clack's suite from outside the checkout, once per Prompt — and, for `path`, writes down the
   `memfs` volume that suite builds, because a suggestion list has nothing behind it otherwise
   ([ADR-0010](./docs/adr/0010-the-recorder-instruments-clacks-suite-from-outside-it.md),
   [ADR-0035](./docs/adr/0035-a-path-is-an-autocomplete-whose-filter-is-a-filesystem.md)), and `node
   scripts/harvest-authored.mjs` runs `scripts/authored/cases.mjs`, which has no upstream snapshot
   behind it and so is guarded differently
   ([ADR-0016](./docs/adr/0016-hand-authored-scenarios-and-the-width-clack-actually-wraps-to.md)).
   Two more Recorders cover the output that is not a Prompt, on the same terms: `node
   scripts/harvest-static.mjs` for the renderers that write once
   ([ADR-0029](./docs/adr/0029-the-static-renderers-write-once-and-never-wrap.md)), and `node
   scripts/harvest-scripted.mjs` for the ones driven by calls rather than keys, where a case is a
   script — `start`, ticks of a fake clock, an `advance`, a `stop`, or a task log's `message` and
   `group` — and every step's bytes are written down separately
   ([ADR-0032](./docs/adr/0032-a-spinner-walks-the-cursor-back-over-a-string-it-never-wrote.md)).
   Still missing: emoji and ZWJ sequences at a margin, where the wrap has to decide whether to
   break one; and `password`'s `clearOnError`, which needs a `validate` callback and so cannot be
   recorded at all — it is covered by unit tests either side of the render, which is weaker and is
   the honest ceiling rather than a shortcut
   ([ADR-0018](./docs/adr/0018-a-render-callback-that-writes-and-a-listener-that-closes.md)).

   Two narrower comparisons run beside the Grid and are not redundant with it, because each catches
   mutations the other misses
   ([ADR-0015](./docs/adr/0015-the-emulator-is-avt-and-it-cannot-see-conceal.md)). A Prompt's
   *opening* Frame is the only one clack writes whole rather than as a diff, and every Scenario's is
   asserted against the widget's, as styles — which is the only place conceal is checked, no
   emulator having a model for it. And every Scenario's whole byte stream, styling stripped from
   both sides, is asserted against the stream a `Session` produces, which says the port asked for
   the same work rather than merely arriving at the same screen. M2 gave that last one its clearest
   justification: the two extra sequences a `confirm` writes when it settles from a `y` cancel each
   other out, so deleting them fails the stream comparison and passes the Grid.
2. **Conformance suites** — one per ported primitive, comparing against its JavaScript counterpart:
   `LineEditor` vs Node `readline`, text measurement vs `fast-string-width`, line breaking vs
   `fast-wrap-ansi`, Frame reconciliation vs `@clack/core`'s `render`, option-list windowing vs
   `limitOptions`
   ([ADR-0020](./docs/adr/0020-limit-options-is-ported-against-a-corpus-and-reaches-a-width-of-nothing.md)),
   key parsing (Node `readline`
   vs crossterm — the one still owed a harvest, and so the least-guarded thing in the `text` path).
   The comparison is harvested rather
   than live: CI is one Rust job with
   no JavaScript to run, and `upstream/` is not committed
   ([ADR-0008](./docs/adr/0008-width-parity-is-asserted-against-a-harvested-fixture.md)).
3. **Drift** — `mise run drift` re-runs every Recorder against pinned clack and reports Fixtures that
   no longer match. It refuses a dirty fixtures tree, because the committed recordings are the
   baseline it compares against. Run deliberately, not in CI: it needs the `upstream/` checkout
   that CI does not have, which `mise run upstream` creates — clack at the pinned tag, installed
   and built. The tag itself lives in one place, `scripts/upstream.mjs`, which is also what every
   harvest asks before it records: a Fixture taken from the wrong commit is worse than no Fixture,
   because it fails somewhere unrelated months later.

Validation is `FnMut(Option<&T>) -> Option<String>` plus a `Validator` trait — the `Option` because
upstream runs it against a value that may never have been set, which is how a bare `return` on an
untouched Prompt reaches its validator at all. clack also accepts a
[Standard Schema](https://github.com/standard-schema/standard-schema), which has no Rust analogue;
the trait is the extension point for adapting crates like `garde`.

## Tooling

`mise.toml` tasks, `hk.pkl` pre-commit, `rustfmt.toml` (`hard_tabs`), `.editorconfig` — carried over
from [ardent](https://github.com/idleberg/ardent). CI is a single ubuntu job running those same
three task names — `fmt:check`, `lint`, `test` — and no JavaScript, which is the whole point of
harvesting. `mise run upstream` is the one command between a fresh clone and being able to record;
`mise run drift` is the one that records. Publishing is manual, `clackatui-core` first.

Known gaps, accepted: no Windows or macOS signal on terminal code, and no automated guard against
upstream drift — `mise run drift` is deliberate, because the checkout it needs is not committed.

## Roadmap

| | |
|---|---|
| **M0** | ~~`ForcedWidth` probe — the one experiment the architecture rests on (below)~~ **done** |
| **M1** | ~~`text` end to end — Recorder, width port, `LineEditor`, `TextState`, `Frame`, Theme, `text` widget, wrap port, Emitter, `.interact()`, harvested text Scenarios green, hand-authored Scenarios (narrow, CJK, resize)~~ **done** |
| **M2** | ~~`password` and `confirm` — states, widgets, builders, both suites harvested, eleven more hand-authored Scenarios~~ **done** |
| **M3** | ~~`limit-options` against a 54-case corpus, `select`, `multiselect` and `select-key` end to end with their suites harvested~~ **done** |
| **M4** | ~~group-multi-select~~, ~~autocomplete~~, ~~date~~, ~~multi-line~~ **done** |
| **M5** | ~~`log`, `intro`, `outro`, `cancel`, `note`, `box`, `spinner`, `progress-bar`, `task-log`, `path`~~ **done** |
| **M6** | theme polish, docs, publish |

M1 is one Prompt rather than one layer on purpose. Every decision here assumed Grid parity through an
emulator was achievable, and that assumption is now tested rather than hoped for: `text` runs end to
end and twenty-one Scenarios agree with clack on the Grid — ten of them harvested, eleven written
to reach what a harvest cannot supply, since upstream's tests never vary the terminal. Both of the
divergences the port had written down are now settled by a recording rather than by argument: one
was unobservable and stays, and one was observable, so the port changed
([ADR-0017](./docs/adr/0017-restore-cursor-walks-back-over-rows-that-are-not-there.md)).

M2 was the test of that. The prediction was that a `password` and a `confirm` widget would be the
whole cost, the Recorder and the Fixture shape and the Grid comparison being Prompt-agnostic
already. It held on the infrastructure — harvesting both suites was two commands and no code — and
it was wrong about the widgets, in a way worth writing down: both Prompts reach outside the shape
`text` established. A `confirm` settles from inside its own listener and a `password` mutates itself
from inside its render callback, so `PromptState` grew two members and `Session` grew one
([ADR-0018](./docs/adr/0018-a-render-callback-that-writes-and-a-listener-that-closes.md)), and
`confirm` is the first Prompt to wrap its own message — badly
([ADR-0019](./docs/adr/0019-confirm-wraps-its-message-against-the-length-of-an-escape-sequence.md)).
All three are invisible to upstream's own tests and were found by reading, then settled by a
recording. What M3 should cost is the same bet again, with one addition: `select` and friends are
the first Prompts with a list in them, so `limit-options` has to be ported before any of them can be
drawn at a height.

That port is done, and it cost more than a list-cutting function should. `limitOptions` is pure, so
it takes a corpus rather than a recording — fifty-four cases against upstream's fourteen, because
upstream's never vary the terminal, never set a column padding and never walk a cursor down a list.
Six of seven mutations of the port are caught by the corpus. The seventh was a branch modelling a
`splice` that cannot run off its array, and was deleted rather than covered, which is the other
thing a surviving mutation can mean. What it turned up on the way is a place the wrap port had said
it would not go: `limitOptions` subtracts a padding from the terminal before it wraps, `select`
passes thirteen for the padding, and so a narrow enough terminal reaches the wrap with nothing left
— which upstream handles by dividing by zero and laying every code point on a row of its own. The
wrap now does too
([ADR-0020](./docs/adr/0020-limit-options-is-ported-against-a-corpus-and-reaches-a-width-of-nothing.md)).

`select` followed, and the prediction held this time: twenty harvested Scenarios, no new machinery
except a height — `Session`'s draw callback carries one now, because how much of a list is drawn
depends on it. Thirteen mutations of the port are caught, nine of them by upstream's own recordings.
The one no reading would have found is a cancelled value's strikethrough, which `wrapAnsi` opens once
and closes at the end while reopening the dim row by row — so the Guide bars in between are drawn
struck through, and the port draws them that way too
([ADR-0021](./docs/adr/0021-select-reads-three-widths-and-a-strikethrough-outlives-its-row.md)).

`multiselect` then reused all of it — the same `Option`, the same cursor walk, the same windowing —
and paid for its one new thing. Upstream's `required` check returns a two-line message whose second
line is a styled `Press ␣space␣ to select`, and a Frame holds no escapes at all, so the error is split
where the architecture already cuts: the Prompt carries the sentence, the widget draws the advice from
the Theme. The Grid is identical and the message is now restylable
([ADR-0022](./docs/adr/0022-multiselect-draws-an-error-a-frame-cannot-carry.md)). Mutation testing
found the gap that mattered more than any of that: an error Frame is never the *last* Frame a Scenario
draws, so the Grid comparison never sees one and the stream comparison strips its styles — three
mutations of the error Frame's colours survived every recording upstream has. Unit tests now hold
them.

`selectKey` is the smallest of the three — no windowing, no footer, no validator, and a cursor that
never moves — and it paid for one ordering: it sets `state = 'submit'` and resolves from inside its
own `key` listener, so the cursor is shown before the settled Frame rather than after the newline
that follows it. It also found something worth more than the Prompt. It is the first suite to cancel
on a value long enough to wrap, and it failed the Grid comparison on a single cell. The port was
right and so was the recording: `avt` had been fed `\n` as a bare line feed, where clack writes to a
tty whose output discipline turns every one into a carriage-return line feed. Every Frame had been
drawn down a staircase, and the cells to the left of each row — ones no terminal ever has — were
being compared for the style that happened to be open when they were skipped. The emulator is fed
through the line discipline now
([ADR-0023](./docs/adr/0023-selectkey-resolves-before-it-draws.md)).

M4 opens with `groupMultiselect`, which is `multiselect` with headers in its list and shares four
pieces with the three Prompts before it. Its own contribution is arithmetic: each option is wrapped
against `columns - prefix.length` where the prefix has already been styled, so the same label breaks
nine columns apart depending on whether its branch was dimmed, and then `limitOptions` wraps the row
again at a third width. Upstream's suite is short labels at eighty columns and can see none of it, so
the evidence is two hand-authored Scenarios at forty columns, one putting the same label under both
branches and one under the third prefix that `selectableGroups: false` produces
([ADR-0024](./docs/adr/0024-groupmultiselect-measures-a-prefix-nobody-can-see.md)).

`autocomplete` and `autocompleteMultiselect` come out of one upstream file and one state, and they
are the first Prompts here that both type and navigate. The harvest recorded nothing on its first
run and said so only by writing an empty Fixture: theirs is the one suite that imports past
`src/index.js`, so the Recorder's alias never fired and the shim never saw a prompt. With a second
shim in place, the two turn out to disagree with each other about the bar beside their own options —
one subtracts the three columns it draws, the other subtracts nothing and overruns — and to agree on
drawing the search box's caret wherever the *option* cursor happens to point, which is nowhere near
the letter you just walked back over. Neither is reachable from upstream's tests, so three more
Scenarios were hand-authored: two at forty columns, and one that presses left
([ADR-0025](./docs/adr/0025-autocomplete-slices-the-search-box-at-the-option-cursor.md)).
Thirty-nine mutations of the port, ten of them uncaught until the tests grew to meet them; the four
that survive are equivalent, and each is written down beside the code it could not change.
A fifteenth was answered by deleting the line rather than covering it.

`date` is the thinnest harvest in the port and the largest authored share: upstream's suite is eight
Scenarios and nine keypresses, seven of them a bare `return` on a field that was already filled, so
it types no digit and presses no arrow and the segment editor — three hundred of `DatePrompt`'s four
hundred lines — reaches no recording at all. Ten hand-authored cases are that editor, and they are
the first written for a behaviour rather than for a width. They pin four things clack does that a
port would not invent: a year below 100 is drawn, accepted by every check, and still resolves to
nothing, because `Date.UTC` reads it as 1900 + it; a `defaultValue` is documented as a fallback and
implemented as a seed; a settled Frame prints the *segments* rather than the value, so a Prompt can
answer `2025-12-25` while writing `__/__/____` on the terminal; and a fifth digit into a full year
makes it five characters wide. The Recorder had to learn to write down a `Date` first — four of this
Prompt's options hold one, and both harvesters had been flattening every such object to
`{ opaque: true }`
([ADR-0026](./docs/adr/0026-date-is-three-strings-and-a-calendar-that-refuses-them.md)).

`multiline` closes M4 and is the only Prompt here that keeps a text editor of its own — untracked, so
readline never sees it, and every insertion, deletion and cursor move is its own. Its `return` is a
newline; two in a row at the end of the text submit, and the first one's newline is taken back out on
the way. Upstream's suite sends thirty-eight keypresses, thirty of them a bare `return`: it never
moves the cursor, deletes anything, opens with text in the field, or varies the terminal, so nine
more Scenarios were hand-authored for the editor and the wrap. Three things they pin: the error foot
is drawn whether or not there is a Guide, alone among the Prompts; a settled Frame with no value
still draws a bar and two trailing spaces; and the text is wrapped thirteen columns early for three
columns of bar, which is ADR-0019's defect in its third place. The ninth of those Scenarios narrows
the terminal under text already several rows tall, and disagreed with clack by two rows — because
every widget in the Scenario loader had been measuring against a width captured when the Prompt was
built, which no resize could move
([ADR-0027](./docs/adr/0027-multiline-keeps-an-editor-its-own-suite-never-uses.md)).

One debt outlived every Prompt that incurred it. Every authored resize up to that point moved the
terminal's *width*, because until M3 the width was the only thing a resize could change — but
`limitOptions` sizes its window off the **height**, so a list is the one thing in clack that
re-lays-out when nothing about the width has moved. Upstream's list suites never resize and never set
a height, and `maxItems` is the only lever they pull, which is the lever that bypasses the terminal.
Seven more Scenarios are hand-authored for it: a window that shrinks, a cut window whose start has
already slid, the five-option floor `MINIMUM_ITEMS` holds whatever the terminal says, a window that
grows back, and one each for the three other Prompts that compute their own `rowPadding`. All fifty-three
agree, and this batch corrected nothing — the height was already live everywhere, which is a reading of
the code these recordings turn into a test
([ADR-0028](./docs/adr/0028-a-list-re-lays-out-against-a-height-nothing-had-moved.md)).

M0 came first because it was cheap and load-bearing. Reusing `BufferDiff` under our own width model
depends entirely on `CellDiffOption::ForcedWidth`, which is recent API on a pre-1.0 crate. The probe
placed a symbol whose `fast-string-width` and Ratatui measurements disagree, stamped `ForcedWidth`,
diffed against a previous Buffer, and confirmed trailing-column skipping follows our number rather
than Ratatui's. It does —
[ADR-0006](./docs/adr/0006-depend-on-ratatui-core-for-primitives-only.md) stands. It also turned up
one thing the diff does *not* do: when a forced-wide cell shrinks, the columns it vacates are never
yielded, so the Emitter has to mark them dirty itself
([ADR-0007](./docs/adr/0007-forced-width-holds-but-the-emitter-owns-shrink-repaints.md)). Better
learned in an afternoon than in M4 — though as it turned out, not by the Emitter, which diffs lines
rather than cells and so never meets the gap
([ADR-0013](./docs/adr/0013-the-emitter-diffs-lines-because-clack-does.md)). The stamped cells still
matter to anyone drawing a clackatui Prompt inside their own Ratatui application, which is the other
half of what [ADR-0002](./docs/adr/0002-own-inline-emitter.md) buys.

The symbol it was built on did not survive M1. `wrapAnsi` composes every Frame to NFC before writing
it, and the conjoining jamo the probe used compose to one syllable that both width models measure
alike, so that particular disagreement cannot reach a terminal through clack. The conclusion is
untouched — tabs, emoji sequences and jamo with no composed form still part the two models — but the
example is now a tab
([ADR-0012](./docs/adr/0012-clack-wraps-its-own-frames.md)).

## Open

- Confirm `clackatui` and `clackatui-core` are unclaimed on crates.io.
- A variable-height inline viewport for Ratatui upstream would remove the need for
  [ADR-0002](./docs/adr/0002-own-inline-emitter.md). Worth doing, separately from v1.
