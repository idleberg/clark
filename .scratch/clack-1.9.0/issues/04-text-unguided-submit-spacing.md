# 04: Text: no leading spaces on an unguided submit (clack #599)

**What to build:** A submitted text Prompt drawn without the Guide shows its value flush left. Upstream used to put two spaces before it regardless; now the spaces come only with the Guide bar.

**Blocked by:** 02

**Status:** done

- [x] With `with_guide` off, the submitted value starts in column 0; with it on, the row is unchanged
- [x] Cancel and error rows are not affected
- [x] A unit test covers the unguided submit row
- [x] The text suite passes
