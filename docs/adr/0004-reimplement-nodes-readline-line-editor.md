# Node's readline line editor is reimplemented, not delegated

`text`, `password` and `autocomplete` do no text editing of their own — they read `rl.line` and
`rl.cursor`, so Node's `readline` *is* their Line editor. clackatui therefore inherits readline's
default keymap as a compatibility requirement (`ctrl+a`/`ctrl+e`, `ctrl+b`/`ctrl+f`, `ctrl+u`/
`ctrl+k`, `ctrl+w`, `alt+b`/`alt+f`/`alt+d`, `ctrl+h`, `ctrl+d`, `ctrl+l`, home/end/delete, and its
full-width-aware cursor arithmetic) despite none of it appearing anywhere in clack's source.

## Consequences

`LineEditor` gets its own Conformance suite asserting `(line, cursor)` after each key, separate from
Prompt Scenarios — so a word-boundary disagreement is reported once as a keymap defect rather than
as a dozen unexplained Grid mismatches. `readline` is driven by `scripts/harvest-line-editor.mjs`
rather than by the test, for the reasons ADR-0008 gives; the recording is per keypress, not per
scenario, so a divergence names the key that caused it.

`multi-line` is the exception: it implements its own cursor and editing, and does not use
`LineEditor`.

## What the port leaves out

History navigation and tab completion are not ported. clack builds its interface with no completer
and closes it when the Prompt resolves, so the history a Prompt observes is always empty; `up` and
`down` are inert, which is what readline does with an empty history anyway. Everything that is
display rather than state — `kRefreshLine`, `getCursorPos`, `ctrl+l`, `ctrl+z` — is the Emitter's
business, and a core that performs no I/O has nothing to say about it.

Two upstream branches close the interface instead of editing (`ctrl+c`, and `ctrl+d` on an empty
line), so they have no `(line, cursor)` transition to record. They surface as an `Abort` event and
are covered by unit tests rather than by the fixture.

`rl.cursor` is a UTF-16 offset, because that is what a JavaScript string index is, and clack copies
it straight into `Prompt._cursor`. The port's cursor is a byte offset into a UTF-8 `str` — what a
Rust caller wants — and converts on demand for the Conformance suite. The two agree on *where* the
cursor is: readline steps by code point and never by grapheme, so a combining mark is a separate
stop on both sides, and reproducing that is the point.

One divergence found in the porting is worth naming, because it looks like a transcription slip and
is not: `kWordRight` matches `/^(?:\s+|[^\w\s]+|\w+)\s*/` while `kDeleteWordRight` matches
`/^(?:\s+|\W+|\w+)\s*/`. `\W` includes whitespace and `[^\w\s]` does not, so on `"! !x"` `alt+f`
stops after `"! "` while `alt+d` deletes `"! !"`. Both are in the corpus.

## Considered options

`reedline` and `rustyline` are both well-tested, but they ship emacs/vi keymaps that differ from
readline's in exactly the edge cases under test, and both want to own the terminal and the prompt
display — which a state-machine core that performs no I/O cannot give them.
