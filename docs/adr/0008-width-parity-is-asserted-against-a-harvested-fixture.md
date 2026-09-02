# Width parity is asserted against a harvested fixture, not a live Node

ADR-0005 asks for a Conformance suite that feeds one corpus to both `fast-string-width` and the Rust
port and asserts equal widths. The obvious reading is that `cargo test` shells out to Node. It does
not, and cannot: `upstream/` is a local reference checkout that is not committed, so on CI there is
no JavaScript to compare against. A suite that silently skips when Node is missing would be worse
than none, because it would be green exactly where it matters least.

So the comparison is made once, deliberately, and recorded. `scripts/harvest-width.mjs` runs the
real library over the corpus and writes `clackatui-core/tests/fixtures/width.json`;
`tests/width_parity.rs` asserts the port against that recording, always, with no JavaScript
involved. This is the Recorder-and-Fixture arrangement ADR-0003 already establishes for Prompt
Scenarios, applied to a primitive instead of a Prompt, and it inherits the same Drift problem: the
fixture is only as current as the last harvest.

Corpus entries are lists of code points, never string literals, on both sides. A decomposed sequence
written literally into a source file is silently precomposed somewhere between the editor and the
disk, which changes what is being measured without changing what anyone reads.

## The tables decide most of it

The ported regexes lean on Unicode properties — `\p{Emoji}`, `\p{Emoji_Modifier_Base}`,
`\p{Emoji_Presentation}`, `\p{Script=…}`, `\p{M}`. Upstream reads them from whichever Unicode
version the running V8 was built against; the port reads them from `unicode-properties` and
`unicode-script`. Neither side pins the other, and a table bump moves answers without moving a line
of code.

Both crates are therefore pinned exactly, as `ratatui-core` is, and the fixture records the Unicode
version it was harvested under. A third test compares that against the version
`unicode-properties` was built against and fails when the two part company — not because a mismatch
is wrong in itself, but because it is the first thing to check when parity breaks. At the time of
writing both are Unicode 17.0, which is why 82 of 82 cases agree.

## Consequences

- Refreshing the fixture requires a clack checkout under `upstream/` and is a deliberate act, not
  something CI can do. It belongs with `mise run drift`. `mise run upstream` makes that checkout,
  and `scripts/upstream.mjs` is what every harvest asks whether it is the right one — the pinned
  tag lives there and nowhere else.
- The port is only as correct as the corpus is representative. `tests/width_parity.rs` guards the
  recording itself — a minimum case count, unique names, and a list of cases that must survive,
  one per branch of the scanner — so that a truncated harvest cannot pass for free.
- The arrangement generalises, and has been reused: the `LineEditor` Conformance suite ADR-0004 asks
  for is harvested the same way, by `scripts/harvest-line-editor.mjs`, for the same reason and with
  the same guards — though there the missing checkout is not the obstacle, since `readline` is a
  Node builtin; CI simply has no JavaScript step, and adding one to run a test that can only ever
  confirm a recording is not worth the second toolchain. It records a transition per keypress rather
  than one answer per case, because a keymap defect is easier to read when the fixture names the key
  that caused it.
- Truncation is not ported. `fast-string-width` is a wrapper over `fast-string-truncated-width`
  called with no limit, and only the width half has a consumer today. The other half returns a
  UTF-16 index into the input, which has no honest Rust counterpart; that question is deferred to
  whoever ports `fast-wrap-ansi`, who will have a caller to design against.
