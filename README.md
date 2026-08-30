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
Node's `readline` on all 493 keypresses of its own. See
[CONTEXT.md](./CONTEXT.md) for the vocabulary and [docs/adr/](./docs/adr/) for the decisions behind
the shape below.

## Compatibility target

| | |
|---|---|
| clack | `@clack/prompts@1.7.0`, `@clack/core@1.4.3` (published; pinned by lockfile) |
| Ratatui | `ratatui-core` 0.1.2 (pinned exactly — pre-1.0, expect churn) |
| Terminal I/O | crossterm 0.29 |

## Shape

Two crates:

- **`clackatui-core`** — state machines and `Widget` impls over `ratatui-core`. No I/O. Feed it a
  key event, render it into a `Buffer`.
- **`clackatui`** — a blocking driver plus clack's sugar (intro, outro, log, note, box, group). No
  async runtime imposed.

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
`Theme::clack()`, ported from clack's `common.ts` with its `is-unicode-supported` and `FORCE_COLOR`
sniffing replicated.

One scope simplification worth recording: clack runs its prompt tests twice, under `CI=true` and
`CI=false`, but `CI` only affects `spinner.ts` and `task-log.ts`. Both are v2, so v1 needs one pass,
not two.

## Testing

Three layers.

1. **Prompt Scenarios** — harvested from clack's own test suite, plus hand-authored coverage of what
   upstream never varies: narrow and wide terminals, mid-Prompt resize, CJK and emoji input, long
   values. `cargo test` replays both the recorded Fixture and live clackatui output through one
   emulator (`vt100`; `avt` as fallback) and compares Grids.
2. **Conformance suites** — one per ported primitive, comparing against its JavaScript counterpart:
   `LineEditor` vs Node `readline`, text measurement vs `fast-string-width`, key parsing (Node
   `readline` vs crossterm). The comparison is harvested rather than live: CI is one Rust job with
   no JavaScript to run, and `prior-art/` is not committed
   ([ADR-0008](./docs/adr/0008-width-parity-is-asserted-against-a-harvested-fixture.md)).
3. **Drift** — `mise run drift` re-runs the Recorder against pinned clack and reports Fixtures that
   no longer match. Run deliberately, not in CI.

Validation is `FnMut(&T) -> Option<String>` plus a `Validator` trait. clack also accepts a
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
| **M1** | `text` end to end — Recorder, ~~width port~~, ~~`LineEditor`~~, Emitter, `TextState`, `.interact()`, harvested text Scenarios green |
| **M2** | password, confirm |
| **M3** | select, multi-select, select-key |
| **M4** | group-multi-select, autocomplete, date, multi-line |
| **M5** | static renderers |
| **M6** | theme polish, docs, publish |

M1 is one Prompt rather than one layer on purpose. Every decision here assumes Grid parity through an
emulator is achievable, and until a Prompt runs end to end that assumption is untested.

M0 came first because it was cheap and load-bearing. Reusing `BufferDiff` under our own width model
depends entirely on `CellDiffOption::ForcedWidth`, which is recent API on a pre-1.0 crate. The probe
placed a symbol whose `fast-string-width` and Ratatui measurements disagree, stamped `ForcedWidth`,
diffed against a previous Buffer, and confirmed trailing-column skipping follows our number rather
than Ratatui's. It does —
[ADR-0006](./docs/adr/0006-depend-on-ratatui-core-for-primitives-only.md) stands. It also turned up
one thing the diff does *not* do: when a forced-wide cell shrinks, the columns it vacates are never
yielded, so the Emitter has to mark them dirty itself
([ADR-0007](./docs/adr/0007-forced-width-holds-but-the-emitter-owns-shrink-repaints.md)). Better
learned in an afternoon than in M4.

## Open

- Confirm `clackatui` and `clackatui-core` are unclaimed on crates.io.
- A variable-height inline viewport for Ratatui upstream would remove the need for
  [ADR-0002](./docs/adr/0002-own-inline-emitter.md). Worth doing, separately from v1.
