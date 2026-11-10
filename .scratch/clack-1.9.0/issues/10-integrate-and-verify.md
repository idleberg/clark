# 10: Integrate and verify 1.9.0

**What to build:** Close out the 1.9.0 integration branch. Fold in the one remaining small behaviour change, confirm the changes that need no code, and prove the branch is clean against pinned clack.

**Blocked by:** 03, 04, 05, 06, 07, 08, 09

**Status:** done

- [x] multiselect and groupMultiselect toggle the focused option on an unnamed key whose character is U+3000 (an IME-committed ideographic space), with unit tests (clack #608)
- [x] A test shows an empty-string autocomplete initial value is kept (clack #618), and the limit-options corpus covers falsy options (clack #628); code changes only if a test fails
- [x] Notes in the relevant module docs say why clack #630 (measuring the output stream's columns) and #616 (`isCancelled` getter) don't apply to clark
- [x] Remaining mentions of 1.7.0 / core 1.4.3 in the README and docs are updated
- [x] A second `mise run drift` reports `drift: none` (every harvest re-run against a snapshot of the fixtures, `diff -r` empty; the literal task needs the fixtures committed first)
- [x] `mise run test`, `mise run lint` and `mise run fmt:check` all pass
