# 03: Wrap by the prefix's visible width (clack #631)

**What to build:** `wrapTextWithPrefix` now measures the prefix with `stringWidth` instead of `.length`, so a Guided Prompt no longer wraps its text against the length of the prefix's escape sequences. With the Guide shown, confirm, select, multiselect, selectKey, groupMultiselect and multiline wrap 10 columns wider than before; groupMultiselect's dimmed group prefix loses its escape allowance. The instruction footers of select, multiselect and groupMultiselect now wrap with their prefix too, so at narrow widths the footer spans several rows and the list gets fewer. `limitOptions`' column padding keeps its old value: upstream still passes `prefix.length` there.

**Blocked by:** 01, 02

**Status:** done

- [x] Every wrap that previously subtracted 13 for the Guide prefix subtracts the prefix's visible width
- [x] The dimmed-prefix allowance in groupMultiselect is gone from its wrap width
- [x] select, multiselect and groupMultiselect footers wrap with the Guide prefix, and the row count fed to `limitOptions` includes every footer row
- [x] Column padding passed to `limitOptions` is unchanged
- [x] Each Prompt module has a unit test at a width where the old and new wrap disagree
- [x] ADR-0019, ADR-0024 and ADR-0027, and the module docs that describe the escape-length quirk, are annotated as a Lapse naming `@clack/core@1.6.0`
- [x] The confirm, select, multi-select, group-multi-select and multi-line suites pass

## Notes

- **`WRAP_PREFIX_WIDTH` is now 3.** It stays a constant: Frames carry no escapes to measure (ADR-0011). `COLUMN_PADDING` stays 13.
- **groupMultiselect wraps against `width(prefix) + 1`.** That drops `DIM_ESCAPES` and also the `groupSpacing` newline charge, since `stringWidth` gives control characters zero width. The module docs and ADR-0024 say so.
- **One shared footer.** `select::instruction_footer` mirrors upstream's `formatInstructionFooter`. `multi_select::footer` calls it, and groupMultiselect reuses that. `multi_select::INSTRUCTION_SEPARATOR` is gone.
- **ADR-0021 annotated too.** It repeated the same "ten columns lost twice" claim for `select`.
- **Authored Scenario names kept.** `narrow › a group option wraps differently under a dimmed branch` and the `confirm` early-wrap Scenario now record the corrected wrap under their old names. Renaming them means re-recording the fixture.
- **Knock-on in `clark`.** `multi_line::tests::the_field_is_wrapped_against_the_terminal_and_follows_a_resize` resizes to 13 columns rather than 23, because the text no longer wraps at 23.
- **Parity after this ticket.** Every ticket-03 Scenario from 02's work list passes. The remaining failures all belong to tickets 04–09.
