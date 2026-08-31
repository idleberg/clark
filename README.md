# clackatui

A Rust adaptation of [clack](https://github.com/bombshell-dev/clack), built on
[Ratatui](https://ratatui.rs), whose appearance is verified against the JavaScript original rather
than merely modelled on it.

Status: **M0 done, M1 under way.** The `ForcedWidth` probe passed, so the architecture below holds —
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
streams go through one emulator: **all ten agree on the Grid — characters, styles and cursor
position.** That is the comparison [ADR-0001](./docs/adr/0001-parity-is-measured-on-the-grid.md) is
built around and the thing M1 existed to reach. One requirement in it cannot be met — no emulator
models conceal, which is half of how clack draws its text cursor — so the Grid runs beside the
Frame-level style comparison rather than replacing it
([ADR-0015](./docs/adr/0015-the-emulator-is-avt-and-it-cannot-see-conceal.md)). Upstream's tests
never vary the terminal, so seven more Scenarios are hand-authored against the same clack — 40 and
20 columns, CJK text, a wrap that grows as a value is typed and shrinks again as it is deleted — and
**all seventeen agree**. Recording them turned up the width clack really wraps to, which is not the
one the Recorder had been writing down
([ADR-0016](./docs/adr/0016-hand-authored-scenarios-and-the-width-clack-actually-wraps-to.md)). What
is left of M1 is the resize: a Scenario carries one terminal size for its whole life, so the second
divergence [ADR-0014](./docs/adr/0014-the-sequence-is-a-compatibility-surface.md) records has no
recording behind it either way.
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
- **`clackatui`** — a blocking driver plus clack's sugar (intro, outro, log, note, box, group). No
  async runtime imposed. Small on purpose: raw mode, a read loop, the crossterm-to-`readline` key
  decoder, and the builders. The order a Prompt is asked in is in the core, not here
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

**v1** — the 11 interactive Prompts (text, password, confirm, select, multi-select, select-key,
group-multi-select, autocomplete, date, multi-line) and the static renderers (log, note, box, intro,
outro, cancel, group, limit-options).

**v2** — spinner, progress-bar, task, task-log, stream, path.

The v2 set is deferred because it is non-deterministic, not because it is unimportant: `spinner.ts`
is the only clack module touching timers, and `progress-bar` and `task` are built on it; `path.ts`
reads the real filesystem. `Clock` and `Fs` abstractions exist from the first commit so that v2 is
additive rather than a refactor through every module — as does `Theme`, whose `Default` is
`Theme::clack()`, ported from clack's `common.ts`. `is-unicode-supported` is ported with it, as
`Theme::detect()`. `FORCE_COLOR` is not: upstream suppresses colour by having `styleText` return the
string unchanged, and a Frame here carries a `Style` rather than escapes, so the equivalent seam is
where Styles become bytes — the Emitter's, not the Theme's.

One scope simplification worth recording: clack runs its prompt tests twice, under `CI=true` and
`CI=false`, but `CI` only affects `spinner.ts` and `task-log.ts`. Both are v2, so v1 needs one pass,
not two.

## Testing

Three layers.

1. **Prompt Scenarios** — harvested from clack's own test suite, plus hand-authored coverage of what
   upstream never varies: narrow terminals, CJK input, long values. `cargo test` replays both the
   recorded Fixture and the port's own bytes through one emulator (`avt`) and compares Grids —
   characters, styles and cursor position. Two Recorders write the Fixtures, both refusing unless
   the clack checkout is at the pinned tag: `node scripts/harvest-scenarios.mjs text` runs clack's
   suite from outside the checkout
   ([ADR-0010](./docs/adr/0010-the-recorder-instruments-clacks-suite-from-outside-it.md)), and `node
   scripts/harvest-authored.mjs` runs `scripts/authored/cases.mjs`, which has no upstream snapshot
   behind it and so is guarded differently
   ([ADR-0016](./docs/adr/0016-hand-authored-scenarios-and-the-width-clack-actually-wraps-to.md)).
   Still missing: a mid-Prompt resize, which needs a shape the Fixture does not have yet, and emoji
   at a margin.

   Two narrower comparisons run beside the Grid and are not redundant with it, because each catches
   mutations the other misses
   ([ADR-0015](./docs/adr/0015-the-emulator-is-avt-and-it-cannot-see-conceal.md)). A Prompt's
   *opening* Frame is the only one clack writes whole rather than as a diff, and every Scenario's is
   asserted against the widget's, as styles — which is the only place conceal is checked, no
   emulator having a model for it. And every Scenario's whole byte stream, styling stripped from
   both sides, is asserted against the stream a `Session` produces, which says the port asked for
   the same work rather than merely arriving at the same screen.
2. **Conformance suites** — one per ported primitive, comparing against its JavaScript counterpart:
   `LineEditor` vs Node `readline`, text measurement vs `fast-string-width`, line breaking vs
   `fast-wrap-ansi`, Frame reconciliation vs `@clack/core`'s `render`, key parsing (Node `readline`
   vs crossterm — the one still owed a harvest, and so the least-guarded thing in the `text` path).
   The comparison is harvested rather
   than live: CI is one Rust job with
   no JavaScript to run, and `prior-art/` is not committed
   ([ADR-0008](./docs/adr/0008-width-parity-is-asserted-against-a-harvested-fixture.md)).
3. **Drift** — `mise run drift` re-runs the Recorder against pinned clack and reports Fixtures that
   no longer match. Run deliberately, not in CI.

Validation is `FnMut(Option<&T>) -> Option<String>` plus a `Validator` trait — the `Option` because
upstream runs it against a value that may never have been set, which is how a bare `return` on an
untouched Prompt reaches its validator at all. clack also accepts a
[Standard Schema](https://github.com/standard-schema/standard-schema), which has no Rust analogue;
the trait is the extension point for adapting crates like `garde`.

## Tooling

`mise.toml` tasks, `hk.pkl` pre-commit, `rustfmt.toml` (`hard_tabs`), `.editorconfig` — carried over
from [ardent](https://github.com/idleberg/ardent). CI is a single ubuntu job: `fmt:check`, `lint`,
`test`. Publishing is manual, `clackatui-core` first.

Known gaps, accepted: no Windows or macOS signal on terminal code, and no automated guard against
upstream drift.

## Roadmap

| | |
|---|---|
| **M0** | ~~`ForcedWidth` probe — the one experiment the architecture rests on (below)~~ **done** |
| **M1** | `text` end to end — ~~Recorder~~, ~~width port~~, ~~`LineEditor`~~, ~~`TextState`~~, ~~`Frame`~~, ~~Theme~~, ~~`text` widget~~, ~~wrap port~~, ~~Emitter~~, ~~`.interact()`~~, ~~harvested text Scenarios green~~, ~~hand-authored Scenarios (narrow, CJK)~~, resize Scenarios |
| **M2** | password, confirm |
| **M3** | select, multi-select, select-key |
| **M4** | group-multi-select, autocomplete, date, multi-line |
| **M5** | static renderers |
| **M6** | theme polish, docs, publish |

M1 is one Prompt rather than one layer on purpose. Every decision here assumed Grid parity through an
emulator was achievable, and that assumption is now tested rather than hoped for: `text` runs end to
end and seventeen Scenarios agree with clack on the Grid — ten of them harvested, seven written to
reach the widths a harvest cannot supply, since upstream's tests never vary the terminal. What is
left of M1 is the resize divergence
[ADR-0014](./docs/adr/0014-the-sequence-is-a-compatibility-surface.md) records: upstream re-wraps
the previous Frame to count the rows it walks back over and the Emitter keeps the rows it laid out,
which agree unless the terminal narrows. A Scenario carries one terminal size for its whole life, so
settling it needs a resize to become an event in the Fixture rather than a number at the top of it.

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
