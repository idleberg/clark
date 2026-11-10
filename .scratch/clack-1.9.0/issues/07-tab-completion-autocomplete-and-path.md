# 07: Tab completion for autocomplete and path (clack #594, #627)

**What to build:** autocomplete gains an opt-in `complete_on_tab` option (single-select only). Tab replaces the search input with the focused option's value, as if typed, and leaves navigation mode; the footer gains `Tab: complete` between the arrows and `Enter:`. path always turns it on, so Tab descends into directories. path also treats a trailing `\` like a trailing `/` when deciding what to list and what prefix to filter by, on every OS. On Unix this changes which siblings are listed for input that ends in `\`. clark-cli gets a `--complete-on-tab` flag for autocomplete.

**Blocked by:** 02

**Status:** done

- [x] Tab with the option on fills the input with the focused value; with it off, or in multiple mode, Tab behaves as before
- [x] The footer hint appears only when the option is on
- [x] path enables it unconditionally
- [x] Trailing `\` is handled like `/` in the directory test and in the prefix strip
- [x] Unit tests cover the Tab branch and the backslash cases
- [x] clark-cli has `--complete-on-tab`, documented in its README
- [x] The autocomplete and path suites pass
