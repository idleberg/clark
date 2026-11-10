# 01: Separate the wrap width from `limitOptions` column padding

**What to build:** A behaviour-preserving prefactor. One number, 13, currently does two jobs: it is the width a Prompt takes off before wrapping (`wrapTextWithPrefix`'s `prefix.length`), and it is the `columnPadding` handed to `limitOptions`. clack 1.9.0 changes the first and keeps the second, so give them two names and route every use through the right one. Nothing on screen changes, and this lands on `main` against the 1.7.0 Fixtures.

**Blocked by:** None (can start immediately)

**Status:** done

- [x] Every wrap that subtracts the Guide prefix reads a "wrap prefix width" value; every `limitOptions` call reads a separate "column padding" value
- [x] Both values are still 13 with the Guide and 0 without it, and groupMultiselect's dimmed-prefix allowance is unchanged
- [x] `mise run test` and `mise run lint` pass with the 1.7.0 Fixtures unchanged
