# clark owns its inline Emitter rather than using ratatui::Terminal

`Viewport::Inline(height)` fixes its height at construction — `Terminal::resize` reinterprets the
*terminal* size and recomputes the viewport's origin, but there is no API to change the height. clack
Frames change height constantly: a validation error adds a line, `select` expands, `autocomplete`
filters its results. So widgets render into a `Buffer` sized to the Frame's natural height, and a
clark-owned Emitter diffs consecutive Buffers and writes the cursor and erase sequences itself.

## Consequences

Ratatui is a composition layer here — `Buffer`, `Cell`, `Style`, the `Widget` traits — not the
renderer, and not `Layout` or `ratatui-widgets` either (ADR-0006). `clark-core` depends on
`ratatui-core` alone and performs no I/O; the `Widget` impls remain genuine, so clark Prompts
still render inside someone else's Ratatui application.

~~The Emitter is not writing a diff from scratch: it consumes `Buffer::diff_iter` and is responsible
only for turning those cell updates into cursor movement, erasure and SGR transitions.~~
**Superseded by ADR-0013.** Upstream diffs whole lines, not cells, and derives every cursor movement
from line counts — and the branch it takes is observable in the cursor's final column, so the
Emitter reproduces the algorithm rather than reconciling cells. The decision above is unaffected,
and is if anything better supported: `Terminal` could not have produced those sequences either.

## Considered options

Reserving an inline viewport at worst-case height wastes rows and diverges from clack's scrollback
and cursor position whenever the Frame is shorter — which is most of the time. Reconstructing the
`Terminal` on each height change clears the double buffer, forcing a full repaint and flicker on
every keystroke that changes height. Adding a variable-height viewport to ratatui upstream is the
better long-term answer and remains worth doing, but it would block v1 on someone else's review
cycle.
