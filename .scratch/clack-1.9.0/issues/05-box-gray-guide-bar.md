# 05: Box: gray Guide bar (clack #588)

**What to build:** `box` with the Guide on draws its left-margin bar in gray, matching the Guide drawn by log, note and the spinner. It used to be the one border character drawn without a style.

**Blocked by:** 02

**Status:** done

- [x] The box's Guide bar carries the Theme's Guide style; borders and content are unchanged
- [x] A unit test asserts the bar's style
- [x] If ADR-0031 or the box module docs say the prefix is drawn plain, that claim is annotated as a Lapse
- [x] The static box Fixtures pass
