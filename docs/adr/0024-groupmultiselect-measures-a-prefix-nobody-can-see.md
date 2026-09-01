# `groupMultiselect` measures a prefix nobody can see

The fourth list Prompt and the last one built on `multiselect`. A map of group name to options is
flattened into a single list — a row for the group, then a row for each option under it — and the
cursor walks all of them. The selection, the validator, the instruction footer, the error Frame and
the sliding window are `multiselect`'s and are imported rather than written again. What is new is
the flattening, a `space` on a header that ticks a whole group, the branch drawn down the left of
each group, and the arithmetic behind where that branch lets a label break.

## A list whose rows are not all the same kind

Upstream flattens to `{ value: key, group: true, label: key }` for a header and `{ ...opt, group:
key }` for an option, so a header's *value* is the group's name. That makes two of its comparisons
heterogeneous: `cursorAt` is matched against every row's value, and so is the selection. In
JavaScript both are `===` between a string and whatever `Value` is, and both are false for every
`Value` that is not the very string naming a group.

Here they cannot be written at all: a `Row::Group` holds no `T`. So they are simply the false they
almost always are, and the one case where the two part company — a `Value` of `String` equal to one
of the group names — is recorded rather than emulated. Nothing in either suite goes there, and a
Prompt that did would be answering with a group name it can never select.

## The prefix is measured with its escapes, and then twice more

Each option goes through `wrapTextWithPrefix`, which wraps to `columns - prefix.length` — and the
prefix it is handed has already been through `styleText`. A dim `│ ` is two columns and eleven
characters. So an option's label breaks at `columns - 12`, except under the group the cursor is on,
whose prefix is the one passed *unstyled*: that one breaks at `columns - 3`. Nine columns of
difference, carried entirely by escapes that draw nothing. It is ADR-0019's defect again, in a place
ADR-0019 could not see, and `groupSpacing`'s newlines are charged to the same subtraction.

Then `limitOptions` wraps the finished row again, at `columns - 13`, prefix included. So a group
option is wrapped twice at two different widths, and which of the two breaks a given label depends
on how long it is. Upstream's suite cannot see any of this — every one of its 21 Scenarios is short
labels at eighty columns — so the evidence is two hand-authored recordings.
`narrow › a group option wraps differently under a dimmed branch` puts the same label under both
branches at forty columns and shows it breaking in two different places;
`narrow › an unselectable group wraps its option back to the margin` covers the third prefix, the
one `selectableGroups: false` produces, where `prefixEnd` is left at the empty string it was
declared with and a wrapped option's later rows sit a column to the *left* of its first. Both agree
with clack byte for byte, and the unit tests' numbers are read off them rather than derived.

## Three more reproduced rather than corrected

- **An empty group is drawn ticked.** `isGroupSelected` is `items.every(…)`, which is true of no
  items. `space` on such a group ticks nothing and leaves it ticked.
- **A submitted Prompt and a cancelled one ask different questions of the same emptiness.** Submit
  counts the chosen options; cancel trims the joined labels, so an option labelled with spaces reads
  as nothing chosen. And the two spaces after the Guide belong to the *value* on a submitted Frame
  and to the *bar* on a cancelled one — so submitting nothing draws a bare bar and cancelling
  nothing draws a bar with two spaces after it. Both are invisible on the Grid and both are in the
  byte stream, which is why the port has two oracles.
- **Neither branch wraps.** `groupMultiselect` is the one list Prompt whose settled value is written
  as a single row however long it is, and left for `Prompt.render` to wrap.

## Consequences

- `select::plain_title` — the unwrapped title `selectKey` already had — was promoted out of that
  widget, and `multi_select`'s `footer` and `error_footer` became free functions of their own module.
  Three pieces shared rather than copied; the fourth list Prompt is mostly the first three.
- The Scenario loader now reads an `options` **map**, so `serde_json` is built with
  `preserve_order`: a `groupMultiselect` draws its groups in the order they were written, and a
  `BTreeMap` would have sorted them behind the port's back. No Fixture would have noticed yet, which
  is the reason to fix it now rather than when one does.
- Upstream's `values can be non-primitive` case gives two options `Symbol()` values, which JSON
  cannot carry — the recording has labels and no values. The loader gives such an option a value
  nothing else can equal, which is what a `Symbol` is.
- Still owed, and now for four Prompts: a hand-authored Scenario that resizes a list.
