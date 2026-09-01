# A list re-lays-out against a height nothing had moved

The debt carried at the foot of every ADR since
[ADR-0021](./0021-select-reads-three-widths-and-a-strikethrough-outlives-its-row.md) — "a
hand-authored Scenario that resizes a list", owed for one Prompt then and for seven by
[ADR-0027](./0027-multiline-keeps-an-editor-its-own-suite-never-uses.md) — is paid here. Seven cases
in `scripts/authored/cases.mjs`, recorded against the same clack at the same tag, and all seven agree
on the Grid first time.

## Width moves a wrap; height moves a window

Every authored resize before these moved the width, because until M3 the width was the only thing a
resize could change. A list is different. `limitOptions` sizes its window off the terminal's
**height**, less whatever the Prompt keeps back for its own title and footer, so a list is the one
thing in clack that re-lays-out when nothing about the width has moved at all — and the paths it
re-lays-out through are the ones
[ADR-0020](./0020-limit-options-is-ported-against-a-corpus-and-reaches-a-width-of-nothing.md) ported
against a corpus rather than against a recording. The corpus says `limitOptions` computes the right
window. It says nothing about whether the window is recomputed when the terminal moves under it,
because a corpus has no terminal.

Upstream's list suites cannot reach any of it either: they never resize, and they never set a height,
so `output.rows` is the mock's twenty for the whole of every one of them. `maxItems` is the only lever
they pull, and it is the lever that bypasses the terminal.

Four paths therefore had no recording behind them, and the four `select` cases are them:

- **The window shrinks.** Twelve options in eighteen rows of window is a whole list; at ten terminal
  rows the window is smaller than the list, an overflow row appears, and the Frame ends up *shorter*
  than the one being walked back over. That is the direction
  [ADR-0016](./0016-hand-authored-scenarios-and-the-width-clack-actually-wraps-to.md) records the
  divergence in, reached here without a character being typed.
- **A cut window whose start has already slid.** The same shrink with the cursor nine options down,
  so `start` is recomputed from a window it did not have when it was last computed and both overflow
  rows are live at once.
- **The five-option floor.** `MINIMUM_ITEMS` is five whatever the terminal says, so at six rows the
  window is still five options tall and overruns the terminal it is drawn into. The clamp has been in
  `limit_options.rs` since M3 with upstream's comment beside it; nothing recorded had ever seen clack
  apply it, because applying it needs a terminal upstream's suite has no way to ask for.
- **The window grows back.** Cut at ten rows, whole again at twenty-four, so the Frame gains rows
  between two renders with no keypress between them.

## Each Prompt's own padding is a separate claim

`rowPadding` is computed per Prompt and differently in each: `select` and `multiselect` from their
title and footer, `groupMultiselect` the same over a list whose group headers are rows of the window,
`autocomplete` from a header that counts its search box. "The height follows a resize" is therefore
four claims, not one, and the last three cases are the other three — one each, doing nothing but
moving the height under that Prompt.

The `autocomplete` case types a character first, so the height moves under a list a search has
already narrowed. Every label in the shared twelve-option list carries a `c` for that reason: the
filter is not what is under test, and a search that cut the list to two would leave nothing for the
height to cut.

## Nothing diverged, and that is the finding

Seven recordings, seven agreements, no correction to the port. It is worth writing down because it is
not what the last two authored batches did — ADR-0027's ninth case disagreed by two rows and found the
loader measuring against a width no resize could move, and ADR-0016's resizes found upstream walking
back over rows it never drew. The reason this batch found nothing is visible in the fix ADR-0027 made:
`Scenario::stream_width` decided the width question once for every arm, and every list arm already
called `.with_rows(rows)` *inside* its render closure rather than capturing a height at build time. The
height was already live. These cases are what turns that from a reading of the code into a recording.

They are not vacuous, which is a separate question and one worth checking rather than assuming: pinning
the `select` arm's height to a constant fails two of the four immediately, and each case's recording was
checked to contain an overflow row after its resize and not before.

## Consequences

- **`resize` takes a height.** `scripts/authored/cases.mjs` had `resize(columns)`; it is now
  `resize(columns, rows)` with the height optional, so the ten width-only cases above it are unchanged.
  Nothing else moved: `record.test.mjs` already wrote `event.rows ?? output.rows` into every resize
  event, the Fixture already carried it, and `scenarios/mod.rs` already split both streams on it. The
  harness for this had been complete since M1 and only the cases were missing.
- **Every list case holds the width still at eighty.** A wrap and a window moving in the same recording
  would make a disagreement ambiguous about which one caused it, and the labels are short enough at
  eighty columns that none wraps.
- **Fifty-three authored Scenarios.** Seven more than ADR-0027 left, and the first that move a
  terminal's height rather than its width.
- **Nothing is owed.** The debt note carried at the foot of ADR-0021 through ADR-0027 ends here.
