# The emulator is `avt`, and no emulator can see conceal

ADR-0001 makes the Grid the unit of comparison and states a requirement on whatever produces it:

> The emulator must model every attribute clack uses. The text cursor is drawn as
> `\x1b[7m\x1b[8m_\x1b[28m\x1b[27m` — reverse *and* conceal — so an emulator that ignores conceal
> would silently pass every text Prompt for the wrong reason.

No emulator on crates.io meets that requirement. Both candidates were read rather than trusted:

| | `vt100` 0.16.2 | `avt` 0.18.0 |
|---|---|---|
| dim (SGR 2) — a submitted value | yes | yes |
| strikethrough (SGR 9) — a cancelled one | **no** | yes |
| reverse (SGR 7) — the text cursor | yes | yes |
| conceal (SGR 8) — the text cursor | **no** | **no** |
| colour, wide-character occupancy, cursor position and visibility | yes | yes |

So the emulator is `avt`, which models three of clack's four attributes rather than two. README's
"`vt100`; `avt` as fallback" is inverted by this and has been corrected: what made `vt100` the
default was familiarity, and what makes `avt` the choice is that it can see a cancelled value.

## What is lost, precisely

Conceal is not modelled, so the Grid cannot distinguish `\x1b[7m\x1b[8m_\x1b[28m\x1b[27m` from
`\x1b[7m_\x1b[27m`. Both sides are blind to it equally — the comparison cannot *fail* because of
it — but a port that dropped the conceal would pass this test.

ADR-0001 is right that this matters and wrong that it is fatal, because the Grid is not the only
check on appearance. `scenario_replay.rs` compares every Scenario's opening Frame against the bytes
clack wrote, as `Style` values rather than as a rendered screen, and conceal is a bit in a `Style`
like any other. That comparison is what actually guards the text cursor, and it is why the two
tests sit side by side rather than one replacing the other: the Grid sees every Frame but not
conceal, and the opening-Frame test sees conceal but only the first Frame.

The gap left over is conceal in a Frame that is not the opening one. For `text` that is the empty
placeholder redrawn after a value is deleted back to nothing, and it is small enough to name and
leave. Widening the harvest to compare *every* recorded Frame as styles would close it, and is worth
doing when a Prompt whose later Frames are more interesting than `text`'s arrives.

## The two tests are complementary, and it is measurable

Neither subsumes the other, and both directions were checked by breaking the port on purpose:

- Removing `Modifier::DIM` from the Emitter's SGR table leaves all six checks in
  `scenario_replay.rs` green — the opening Frame is compared as a `Frame`, before the Emitter is
  reached, and the stream comparison strips SGR — and fails the Grid on four Scenarios, naming the
  cell: `intensity: Faint` by clack, `Normal` by the port.
- Swapping the newline and the cursor-show in `Session::close` fails all ten stream comparisons and
  leaves the Grid green, because both orders end with the cursor in the same place and visible. The
  order is observable in the bytes and not in the final Grid, which is exactly the kind of thing
  ADR-0014 argued belongs in the core.

Two tests that catch different mutations are two tests worth keeping.

## Consequences

- `avt` is a dev-dependency of `clackatui-core`, and is the arbiter of every appearance claim in the
  project. It is not pinned exactly, but a bump is read rather than taken: an emulator that quietly
  stopped modelling an attribute would turn a parity claim into a tautology.
- Two tests in `scenario_parity.rs` guard the emulator itself. One asserts it still models the
  attributes the Theme uses, and that picocolors' encoding and the Emitter's land on the same cell.
  The other asserts it still does *not* model conceal — a test that fails when the news is good, so
  that the caveat above is taken out of the docs by a failure rather than by someone remembering.
- The Scenario loader moved into `tests/scenarios/mod.rs`, shared by both test binaries. Two files
  reading one Fixture differently is a bug that would be blamed on the port.
- All ten replayable `text` Scenarios agree on the Grid, cursor included. Every one of them runs at
  80 columns and none resizes, so the wrap and re-layout paths — the two divergences ADR-0014
  records — are still untested. The hand-authored Scenarios are what close that, and this is what
  they will be run through.
