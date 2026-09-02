# clark

A Rust adaptation of [clack](https://github.com/bombshell-dev/clack), built on Ratatui, whose
appearance is verified against the JavaScript original rather than merely modelled on it.

## Language

### The library

**Prompt**:
One interactive question posed to the user — text, password, confirm, select, multi-select,
select-key, group-multi-select, autocomplete, date, multi-line.
_Avoid_: input, question, field

**Static renderer**:
Output that is written once and never revised — intro, outro, cancel, log, note, box. Distinguished
from a Prompt by having no input loop and no Frame history.
_Avoid_: message, non-interactive prompt

**Frame**:
The complete visual state of a Prompt at one instant. Prompts advance by producing a new Frame and
letting the Emitter reconcile it against the previous one.
_Avoid_: render, screen, view

**Guide**:
The vertical bar running down the left margin that visually joins consecutive Prompts into one flow.
clack's own term, carried over.
_Avoid_: bar, gutter, rail

**Theme**:
The symbol set and style palette a Frame is drawn with. `Theme::clack()` is the one whose appearance
is under test; every other Theme is unverified by construction.
_Avoid_: style, skin, palette

**Emitter**:
The component that turns a sequence of Frames into terminal writes. Owns all cursor movement and
erasure, because reproducing clack's inline behaviour requires control Ratatui's `Terminal` does not
expose.
_Avoid_: renderer, backend, writer

**Line editor**:
The single-line text editing model — cursor position, word boundaries, kill and yank — that clack
inherits from Node's `readline` and clark reimplements. A Prompt delegates to it rather than
editing text itself.
_Avoid_: input handler, buffer, textarea

**Cancel**:
The user abandoning a Prompt without answering. A distinct outcome from both a value and a failure:
the surrounding program may legitimately continue afterwards.
_Avoid_: abort, interrupt, quit

### Compatibility

**Parity**:
The property being tested: clack and clark, given the same input, leave the terminal in the same
observable state. Always qualified by which Grid the claim covers.
_Avoid_: compatibility, equivalence, fidelity

**Grid**:
The observable terminal state — characters, styles and cursor position — obtained by replaying an
output stream through a terminal emulator. The unit of comparison, in place of the raw bytes that
produced it.
_Avoid_: screen, buffer, snapshot

**Scenario**:
An input specification: a Prompt's configuration, a sequence of semantic key events, a terminal
size — and, for the one Prompt that reads outside the terminal, the filesystem it read. Deliberately
holds no expected output, so both implementations can be driven from it.
_Avoid_: test case, spec, script

**Fixture**:
clack's recorded output for one Scenario, stored verbatim as bytes. Never interpreted at record
time — interpretation into a Grid happens on the Rust side, so both implementations pass through one
emulator.
_Avoid_: golden file, snapshot, expectation

**Recorder**:
The Node-side tool that drives clack through Scenarios and writes Fixtures.
_Avoid_: generator, driver, capture tool

**Harvest**:
Deriving Scenarios and Fixtures from clack's own test suite rather than authoring them, so that
upstream's accumulated regression history becomes the specification.
_Avoid_: import, scrape, extract

**Drift**:
A Fixture no longer matching what pinned clack currently produces — meaning the recording is stale,
not that clark is wrong. Diagnosed separately from a Parity failure.
_Avoid_: staleness, breakage, regression

**Conformance suite**:
A differential test aimed at one ported primitive — the Line editor, text measurement, key parsing —
comparing it against its JavaScript counterpart directly rather than through a Prompt. Exists so
that a defect in a primitive is reported once, by name, instead of as many unexplained Grid
mismatches.
_Avoid_: unit test, integration test
