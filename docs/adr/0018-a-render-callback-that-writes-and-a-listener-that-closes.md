# A render callback that writes, and a listener that closes

ADR-0009 gives the shape every Prompt here follows: the state machine is `Prompt<S>`, the seven
events clack raises on itself are methods on `PromptState`, and the widget is a pure function from a
`&Prompt` to a `Frame`. `text` fits it exactly. Neither of M2's two Prompts does, and both break it
in a way a terminal can see, so neither can be smoothed over.

## `confirm` settles from inside its own listener

`ConfirmPrompt`'s `confirm` handler is three statements:

```ts
this.output.write(cursor.move(0, -1));
this.value = confirm;
this.state = 'submit';
this.close();
```

`close()` writes a newline, emits `submit`, and unsubscribes. The `submit` listener registered back
in `prompt()` writes `cursor.show` and resolves the promise. And then `onKeypress` carries on: it
emits `key`, skips the `return` check, skips the cancel check, emits `finalize`, calls `render()` —
which still works, `close()` having only torn down readline — and, finding the state is `submit`,
calls `close()` a second time. The second close writes another newline and emits into an empty
subscriber map.

So a `y` produces this, in this order:

```
ESC[1A            the listener's cursor.move(0, -1)
\n                the first close
ESC[?25h          the submit listener, before anything has been redrawn
<the diff>        onKeypress's render, drawing the settled Frame
\n                the second close
```

Every other Prompt writes its settled Frame first and the newline and cursor after it, which is what
`session.rs` was built around and what its module docs describe. Nothing here is deliberate on
upstream's part — the cursor-up predates the render that would have accounted for it — but all five
writes reach the terminal.

**The seams.** `PromptState::CONFIRMS_ON_KEY` is an associated const, next to `TRACKS_INPUT`, and
`ConfirmState` is the only state that sets it. `Prompt::key` reads it immediately after dispatching
`confirm`, at the point upstream's listener runs: it sets `Status::Submit` and raises
`Prompt::closed_early`, then goes on through the rest of `onKeypress` exactly as upstream does — so
a cancel arriving in the same keypress can still override the submit, because upstream's can.
`Session::key` reads `closed_early` and writes the first three sequences before it renders. A second
flag, `Session::resolved`, stands in for `unsubscribe()`: the cursor is shown by the first close and
not by the second.

## `password` clears itself while it is being drawn

`clearOnError` is an option of `@clack/prompts`' `password()`, and it is used inside the `render`
callback:

```ts
const maskedText = masked ?? '';
if (opts.clearOnError) {
    this.clear();
}
return `${title.trim()}\n${errorPrefix}${maskedText}\n…`;
```

The masked text is captured *before* the clear and returned *after* it, so the Frame that reports
the error is the last one with the value in it, and the field is empty by the time the next key
arrives. Get the order wrong in either direction and the recording says so: clear too early and the
error Frame is blank, clear too late and the next Frame still has the old value.

A widget here is handed a `&Prompt` and cannot clear anything. **The seam** is
`PromptState::clears_after_render(status)`, which asks rather than acts, and `Prompt::after_render`,
which acts — called by `Session::render` after the Frame has been composed and handed to the
Emitter. The option therefore lives on `PasswordState` rather than on `PasswordWidget`, which is the
one place M2 moves something out of the widget and into the state; it is there because it changes
the Prompt, and nothing that changes the Prompt can live in a function that cannot.

## Why not simply leave both out

Both are defects and neither is load-bearing. ADR-0013 already settled the rule and this is its
third application: an upstream defect is reproduced where a terminal can see it, because Parity is
about what the terminal ends up holding and not about what anyone meant. The cost of the rule is two
small seams on a trait; the cost of breaking it is that "clackatui looks like clack" acquires a
footnote, and footnotes accumulate.

## What tests it

Neither behaviour is reachable from upstream's own suite — no `confirm` test sends a `y` or an `n`,
and both `clearOnError` tests carry a `validate` callback, which a recording cannot replay. So:

- **`confirm › a y settles the prompt without a return`** and **`confirm › an n settles it after an
  arrow key`**, hand-authored and recorded against the same clack at the same tag. Deleting the
  early close from `Session::key` fails both.
- Worth naming precisely: it fails them in
  `every_scenario_is_written_the_way_clack_wrote_it` and **not** on the Grid. `ESC[1A` followed by a
  line feed is a round trip, so the terminal ends up where it would have anyway. This is the
  clearest evidence so far that the stream comparison is not subsumed by the Grid one.
- **`clearOnError`** has no Scenario and cannot have one: the recording would need the predicate,
  and a Fixture carries none. It is covered by unit tests either side of
  `Prompt::after_render` — the error Frame keeps the value, the state does not — and by a
  `clackatui` builder test that drives a real `Session` through the whole sequence. That is weaker
  than a recording and is the honest ceiling here, not a shortcut.

## Consequences

- `PromptState` grows two members. Both have defaults, so `TextState` is unchanged and a state added
  later never has to mention either.
- `Session` grows `resolved`, which is `unsubscribe()` and will matter again for any later Prompt
  that closes more than once.
- `Session::render` now has a side effect on the Prompt. It is one call, named after what upstream
  does, and it happens at the point upstream's happens; anything driving Frames without a Session
  has to call `Prompt::after_render` itself, and the doc comment says so.
