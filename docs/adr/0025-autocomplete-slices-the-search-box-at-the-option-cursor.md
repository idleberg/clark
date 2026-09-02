# `autocomplete` slices the search box at the option cursor

The fifth and sixth list Prompts, out of one state and one file. `autocomplete` answers with one
option and `autocompleteMultiselect` with several, and both are a `select` with a search box in
front of it: type to narrow the list, walk what is left with the arrows, answer with `return`. It is
the first Prompt in the port that both types and navigates — `super(opts)` without the `false`, so
`trackValue` is on — and it is the first whose list is a *view* of its options rather than the
options themselves.

## Two cursors, and one of them is drawn in the wrong place

`AutocompletePrompt` has a `_cursor`, inherited, which is where the caret sits in the search text,
and a `#cursor`, its own, which is which option is highlighted. The getter `cursor` returns the
second. `userInputWithCursor` uses both:

```ts
if (this._cursor >= this.userInput.length) return `${this.userInput}█`;
const preCursor = this.userInput.slice(0, this.cursor);
const cursorChar = this.userInput.slice(this.cursor, this.cursor + 1);
```

Whether the caret is past the end of the text is asked of the text cursor; where to cut the text is
asked of the list's. They agree only by coincidence. Press left in a search box and the highlight
does not move left — it jumps to whichever character the highlighted option's index happens to point
at, and if the search is shorter than that index the caret vanishes entirely.

Nothing upstream records this: no test in `autocomplete.test.ts` presses left or right. So it is
pinned by a hand-authored recording,
`autocomplete › the search cursor is drawn where the option cursor is` — two matches, the cursor on
the second, the caret walked back to the start, and clack inverts the second character while leaving
the first plain. Reproduced rather than corrected, per ADR-0013: a terminal can see it.

## The same prefix, three widths, and now a fourth

`autocomplete` hands `limitOptions` a `columnPadding` of **3**. That is the honest number — a bar
and two spaces draw three columns — and it makes this the only list Prompt in clack that does not
charge itself for the escapes around them. `select`, `multiselect` and `groupMultiselect` all
subtract thirteen (ADR-0019, ADR-0021, ADR-0024).

`autocompleteMultiselect` passes **no padding at all**. Its options are wrapped as though the bar
beside them were not there, so every row of a long one is three columns too wide and the terminal
breaks it wherever it likes. Two Prompts, one file, one list, and the same option lands in two
different places. The pair of authored recordings at forty columns —
`narrow › an autocomplete option wraps three columns short of the terminal` and
`narrow › an autocompleteMultiselect option overruns its own guide` — put the same two labels under
both and show the break moving.

Neither settled Frame wraps at all, so a long answer is left to `Prompt.render`'s hard wrap.

## Three more reproduced rather than corrected

- **An unguided `autocompleteMultiselect` draws a blank row under its title.** Its header is
  `${title}${hasGuide ? bar : ''}` split on newlines, and `title` ends in one — so with no Guide the
  bar's place is taken by the empty string, which is still a row. `autocomplete` pushes its bar
  inside an `if (hasGuide)` and has no such row. The same two Prompts, the same decision, written
  twice and differently.
- **The two say `required` differently.** `autocompleteMultiselect`'s message is
  `Please select at least one item`, with no full stop; `multiselect`'s is
  `Please select at least one option.`, with one. And the option's default is the opposite way
  round: on for `multiselect`, off here.
- **The search box has a leading space in one Prompt and not the other.**
  `autocomplete` writes the space as part of the text and drops both when the text is empty;
  `autocompleteMultiselect` writes the space unconditionally and then a dim nothing.

## Consequences

- `PromptState` gains one seam, `sets_user_input`. Tab on an empty box types the placeholder for you
  — `_setUserInput(placeholder, true)`, called from inside upstream's own key listener — and a state
  here is handed the text but does not own the editor. So it asks, and `Prompt::key` carries it out
  at the same point in the sequence, before the submit and cancel checks. It is the same
  ask-afterwards shape as `submits_from_key` and `clears_after_render`.
- The Recorder grew a second shim. `autocomplete.test.ts` is the one suite upstream that imports its
  Prompts from `../src/autocomplete.js` rather than from `../src/index.js`, so the alias anchored to
  the entry point never fired and the first harvest recorded nothing at all — silently, with the
  suite passing. `scripts/recorder/autocomplete-shim.mjs` stands in for that module too.
- A `filter` is a callback, and a recording cannot carry one. Three Scenarios install one and all
  three install the same shape — a label that *starts with* the search — so the loader supplies it,
  the way it supplies `multiselect`'s `required` validator. A newer tag that writes a different
  filter does not fail quietly: it fails in parity.
- The single Prompt answers with one value and the state holds a selection, so
  `clark::Autocomplete::validate` adapts — a validator written against `T` is handed the first
  of the `Vec<T>`, which is what `normalisedValue` does on the way out.
- The Grid comparison's anti-vacuity guard was relaxed. It asserted that clack's stream left the
  Scenario's message on the terminal; `autocomplete › renders bottom ellipsis when items do not fit`
  settles into six rows of a terminal five rows tall, and the title scrolls off. The guard now falls
  back to what it is actually for — that clack left *something* to compare against.
- Still owed, and now for six Prompts: a hand-authored Scenario that resizes a list.
