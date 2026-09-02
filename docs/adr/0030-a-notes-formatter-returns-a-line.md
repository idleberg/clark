# A note's formatter returns a Line

M5's second unit: `note`. It is a static renderer like the four in ADR-0029 — one call, one string,
no state and no keypress — and it breaks the one thing they all had in common.

## It wraps, where nothing else static does

ADR-0029's four hand a long line to the terminal and let *it* break, because nothing walks back over
static output and so nothing has to know where the breaks are. A `note` cannot. It draws a border
down the right-hand side, and a border only lands in the right column if the text inside it was
measured first — so the message is wrapped with [`wrap`](../../crates/clark-core/src/wrap.rs), the same
word wrap a Prompt's Frame goes through, at `columns - 6`. Six is exactly what the box costs its
content: the left bar and its two spaces, the two of padding the width gains, and the right bar.

This is also the first thing in the port outside a Prompt that reads the terminal at all, which is
why `note` takes a `columns` and `intro` does not. On the `clark` side that is `terminal::size()`
with upstream's eighty as the fallback.

## `format` returns a `Line`, not a `String`

Upstream's is `(line: string) => string`, and its own tests pass one that adds characters (`* … *`)
and one that adds colour (`styleText('red', …)`). A Frame carries no escapes at all (ADR-0011), so
the colour half of that signature has nowhere to go here.

A formatter here returns a drawn `Line` instead. It covers both cases and it draws the distinction
`wrapWithFormat` needs for free: characters change a Line's width and Styles do not — which is
exactly what upstream gets from `stringWidth`, since `stringWidth` ignores SGR. A caller who wants
red gets red without ever holding an escape sequence.

The corpus records three formatters by name, because a function does not survive JSON: `stars` adds
four columns, `red` adds none, and `red-stars` adds both at once. The recorder holds the upstream
string version and `static_parity.rs` holds the `Line` version, and the Grid is what says they agree.

## Wrapping twice is not an optimisation to remove

`wrapWithFormat` wraps, measures the widest row before and after formatting, and wraps *again* at the
width less the difference. It cannot be folded into one pass: the formatter is only ever given whole
rows, so how much room it wants is unknowable until there are rows to give it.

The second width can reach zero — a formatter that costs as many columns as the terminal has — and
`wrap::breaks` lays a zero-width line out one code point per row, which is what upstream does with it
too. A case in the corpus sits there deliberately. Upstream can go below zero where this cannot, but
the branch that would tell the two apart compares quantities that stay equal either way.

## Consequences

- **`withGuide` is only half applied, and stays that way.** It decides the leading bar and whether
  the bottom-left corner is a `├` or a `╰` — and nothing else. Every row of the box keeps a bar in
  its left margin regardless, so a `note` with the Guide off still draws one down its own side.
  Reproduced rather than tidied, per ADR-0013.
- **No new styles.** The border is the Guide's `gray` and the `◇` is `step_submit`'s green, both of
  which the Theme already had. ADR-0029 added six; this adds none.
- **The corpus grew from thirty cases to forty-four**, and the recorder now carries a `title` and a
  named `format` alongside the message.
- **One branch is kept and is dead.** `Math.max(len - titleLen - 1, 1)`, the floor on the rule beside
  the title, can never be reached: `len` is at least `titleWidth + 2`, so the subtraction is at least
  one already. The mutation pass is what said so — removing the floor changed nothing any case could
  see, and no case could be written that would. It stays because it is what `note.ts` says, and
  because `box` — which truncates its title where `note` does not — is the shape it would have been
  for.
