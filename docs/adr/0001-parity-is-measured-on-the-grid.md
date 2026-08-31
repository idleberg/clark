# Parity is measured on the Grid, not the byte stream

clack's observable output is a stream of ANSI escapes produced by its own line-diffing algorithm, so
asserting byte equality would mean reimplementing that algorithm rather than adapting the library.
We instead replay both implementations' output through a terminal emulator and compare the resulting
Grid — characters, styles and cursor position — after every keystroke. Escape sequences may differ;
what the user sees may not.

## Consequences

Both streams are replayed through **one** emulator, on the Rust side. An earlier shape had the
Recorder interpret clack's output in Node and commit Grids; that puts two different emulators on the
two sides of the comparison, and their disagreements would surface as Parity failures that are
nobody's bug. Fixtures therefore store bytes verbatim.

The emulator must model every attribute clack uses. The text cursor is drawn as
`\x1b[7m\x1b[8m_\x1b[28m\x1b[27m` — reverse *and* conceal — so an emulator that ignores conceal
would silently pass every text Prompt for the wrong reason.
