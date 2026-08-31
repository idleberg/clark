# The Recorder instruments clack's suite from outside it

ADR-0003 makes clack's own ~500 test cases the specification and calls for a Recorder that "clones
clack at tag `@clack/prompts@1.7.0`, patches `MockReadable`/`MockWritable` to log key events and
output chunks, and runs their vitest suite". The patching turned out to be unnecessary, and not
patching is worth the small amount of machinery it costs.

Nothing is written into `prior-art/clack`. `scripts/recorder/vitest.config.mjs` runs upstream's
suite with `root` pointed at the checkout and two things attached: a setup file that owns the record,
and an alias that puts `scripts/recorder/prompts-shim.mjs` in place of `@clack/prompts`' entry point
for the specifier the tests import it by. Re-harvesting at a newer tag is therefore a `git checkout`
and nothing else — no patch to rebase, and no chance of a fixture recorded from a working tree
someone had edited.

The suite passing is part of the recording. Every hook observes and delegates, so upstream's own
snapshots still match while the Recorder runs; if any of them fail, the script writes nothing and
says why. That is the only evidence available that what was recorded is clack, rather than clack
with a recorder in the way.

## Why the shim exists

The obvious hook is `Prompt.prototype.prompt`, which is where clack's I/O happens and would cover
every Prompt including the ones `packages/core`'s tests construct by hand. It does not work, for two
separate reasons.

The first is that `message` never gets there. `text()` closes over it in the `render` callback it
builds and passes the core Prompt only the rest, so a Scenario recorded below that line knows the
keypresses and the frames but not the question. Options have to be caught in the prompt function.

The second is subtler and cost an hour. Under pnpm, `@clack/core` resolves through the workspace
symlink for a file outside the package and through the real path for `packages/prompts/src`, and
Vite treats those as two modules. Patching the class from a setup file patches a `Prompt` that
nothing runs — silently, with the suite still green and every Scenario recording zero keypresses.
The shim sidesteps the question rather than answering it: `input` and `output` are objects the test
itself created, so instrumenting *those* cannot pick the wrong copy of anything.

The one thing the shim does copy is upstream's `vitest.config.ts`, whose `FORCE_COLOR` and ANSI
snapshot serializer the suite needs to pass at all. `scripts/harvest-scenarios.mjs` pins that file's
hash, so a change to it at a newer tag stops the harvest instead of quietly changing what is
recorded.

## What a Scenario cannot carry

Two of the thirteen `text` cases pass a `validate` callback, which does not cross into a recording;
one is driven by an `AbortSignal` rather than by keypresses. They are recorded — the fact that there
was a callback is written down — but replaying them needs a predicate supplied by hand, which is
the parity harness's job and not the fixture's. `tests/scenario_replay.rs` asserts those counts
rather than describing them, so a harvest that turns more of the suite into something unreplayable
is loud.

Recorded strings are stored as strings, not as the code-point arrays ADR-0008 insists on for the
width corpus. The hazard there is an editor precomposing a literal on its way into a source file;
JSON written by the Recorder and read by `serde_json` is not a source file and has no such step.

## Consequences

- `describe.each(['true', 'false'])` runs every case twice, and the Recorder collapses the two runs
  when they are identical. For `text` all thirteen collapse, which turns README.md's claim that `CI`
  only reaches `spinner` and `task-log` into something checked rather than assumed. A case where the
  runs differ is kept under both names.
- The fixture is not yet compared as a Grid, because the Emitter and the widget do not exist. What
  it does support today is a replay of the keypresses through `Prompt<TextState>`, asserting the
  state clack settled in — read off the step symbol in its last frame. ADR-0009 recorded that the
  state machine had no oracle at all; this is a small one, but it is upstream's answer.
- Only the `text` suite is harvested. The mechanism is suite-agnostic and the script takes names as
  arguments, but a fixture for a Prompt that cannot yet be checked is drift with no upside.
- Harvesting refuses unless the checkout is at the pinned tag, rather than recording whatever is
  there and stamping the commit. A fixture from the wrong commit fails somewhere unrelated months
  later, which is the expensive kind of wrong.
