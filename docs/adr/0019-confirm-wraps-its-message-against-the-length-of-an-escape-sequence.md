# `confirm` wraps its message against the length of an escape sequence

ADR-0012 established that clack wraps its own Frames: `Prompt.render` calls
`wrapAnsi(frame, process.stdout.columns, …)` and writes the result, so every break in clack's output
was computed before a byte was written. `text` and `password` have no other wrap. `confirm` does —
it is the first Prompt in the port that breaks its own message before the Frame around it is broken
again — and the width it breaks at is wrong.

## What upstream does

```ts
const titlePrefixBar = hasGuide ? `${styleText('gray', S_BAR)}  ` : '';
const messageLines = wrapTextWithPrefix(opts.output, opts.message, titlePrefixBar, titlePrefix);
```

and, in `@clack/core`'s `utils/index.ts`:

```ts
const columns = getColumns(output ?? stdout);
const wrapped = wrapAnsi(text, columns - prefix.length, { hard: true, trim: false });
```

`prefix` is `titlePrefixBar`, and it is *styled*. It draws three columns — a bar and two spaces —
but as a JavaScript string it is thirteen characters: `ESC [ 9 0 m`, the bar, `ESC [ 3 9 m`, and the
two spaces. `String.prototype.length` counts all thirteen. So a guided `confirm` wraps its message
as though the terminal were ten columns narrower than it is, and its message rows stop well short of
the margin. Without a Guide the prefix is `''`, nothing is mismeasured, and the whole terminal is
used.

There is a second, smaller thing in the same call. `getColumns(opts.output)` reads the width off the
Prompt's *own output stream*, while the Frame around it is wrapped against the global
`process.stdout.columns`. ADR-0016 found that split for the Recorder and pinned the two together;
`confirm` is the first Prompt for which both numbers actually do something. Outside a test harness
they are the same terminal, and `Session` carries the one number and hands it to both.

## The decision

Reproduce it, as `confirm::GUIDE_PREFIX_LENGTH = 13`.

This is ADR-0013's rule again — an upstream defect is reproduced where a terminal can see it — and a
message breaking ten columns early is about as visible as a defect gets. What is different here is
that the port cannot *derive* the number. A Frame carries no escapes at all (ADR-0011); there is no
styled string to take the length of, and there never will be. So thirteen is a constant with the
arithmetic written above it, rather than a measurement of something.

It is thirteen for both Themes, since the ASCII fallback draws the bar with one character too. A
Theme with a longer bar, or one that adds a background colour, would part company with upstream
here — but every Theme other than clack's is unverified by construction, which is what CONTEXT.md
means by the word, so that is a statement about the constant's scope rather than a problem with it.

## What tests it

Nothing upstream can. Every harvested `confirm` Scenario runs at 80 columns with a message of `foo`,
so thirteen and three give identical output and the constant is unconstrained by the whole suite.
Two hand-authored Scenarios, recorded against the same clack at the same tag:

- **`confirm › the message wraps early because the prefix is measured with its escapes`** — 40
  columns, a message long enough to break. Setting the constant to 3 fails it on the Grid.
- **`confirm › an unguided message wraps against the whole terminal`** — the same message, the same
  width, `withGuide: false`. Unaffected by the constant, which is what pins the ten columns to the
  prefix rather than to the wrap.

The pair is the point. Either alone would be satisfied by a port that had the wrap wrong in a
compensating way.

## Consequences

- `Session`'s `Draw` callback now takes the terminal width. `text` and `password` ignore it;
  `confirm` is why it is there. This is a breaking change to a type nothing outside the workspace
  uses yet.
- `ConfirmWidget::with_columns` defaults to 80, which is `getColumns`' own fallback for an output
  that is not a terminal — so a widget built by hand and drawn into someone else's Ratatui layout
  behaves the way clack does with a non-TTY output rather than throwing or wrapping at zero.
- A `confirm` in a narrow terminal will look wrong, and it will look wrong the same way clack does.
