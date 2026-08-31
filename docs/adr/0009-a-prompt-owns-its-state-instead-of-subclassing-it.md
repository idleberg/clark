# A Prompt owns its state instead of subclassing it, and settings are a value

`@clack/core`'s `Prompt` is a base class with an event emitter pointed at itself. `TextPrompt`
*is* a `Prompt` with three listeners attached — `on('userInput')` sets the value, `on('finalize')`
applies the default — and every other Prompt is the same trick with different listeners. The
emitter is not a public extension point that subclasses happen to use; it is the subclassing.

Ported literally, that is a `Map<String, Vec<Box<dyn FnMut>>>` inside a struct that also holds the
things those closures need to mutate, which in Rust is a fight with the borrow checker for no gain.
So `clackatui_core::prompt::Prompt<S>` *owns* an `S: PromptState`, and the seven events a Prompt
raises on itself are methods on that trait. `TextState` overrides two of them and is 60 lines,
which is what `TextPrompt` is.

The events upstream raises for *callers* — `submit`, `cancel`, `value` — are dropped rather than
translated. They carry nothing the accessors do not, and a driver that renders after every key has
already observed everything they would have told it. `prompt()` returning a promise goes with them:
it is the driver's, and the driver is in the other crate.

`onKeypress` itself is ported line for line, in order, because the order is doing work. The Line
editor is driven *first* — upstream registers its listener on an input readline is already reading,
so `rl.line` is current by the time clack looks — and `return` is the one key whose line is not read
back, because readline has already cleared it. Reversing either would quietly lose every answer.

## Settings stop being a global

`utils/settings.ts` is one mutable module object and an `updateSettings()` that merges into it.
Here it is a `Settings` value the Prompt owns, defaulted to clack's. A process-global that every
Prompt reads makes the test suite serial, and the Scenarios ADR-0003 harvests will want to vary the
aliases per Scenario. `updateSettings`' one surprising rule is kept: an alias that already exists is
never overwritten, so a caller cannot take `escape` away from `cancel`.

`settings.actions` is not ported. Upstream builds it once from a `const` array and never mutates it,
so its only role is to ask whether a key name is one of the seven — which an enum answers by
construction. Doing so surfaced that `enter` is `\n` and submission is `\r`: only the former is an
action, so `Enter` never fires on the key that ends a Prompt.

## Consequences

- Two upstream quirks are reproduced knowingly, and are covered by tests that say so.
  `_clearUserInput` sends the Line editor a `ctrl+u` and then blanks `userInput` directly, which are
  not the same operation — anything to the right of the cursor survives in the editor and reappears
  on the next keypress. And a cancel still runs `finalize`, so a cancelled `text` with a
  `defaultValue` ends up holding that default.
- One is not. `ctrl+d` on an empty line closes readline underneath clack without reaching its cancel
  check, leaving the Prompt alive above a dead editor and swallowing every later key. The Line
  editor reports it and the driver ignores it, so the Prompt stays usable. This is a divergence the
  Grid should never see, because there is nothing to draw either way.
- `userInputWithCursor` slices at `cursor + 1` UTF-16 code units, which halves an astral character
  and prints two replacement characters. `InputWithCursor` takes the whole character. Whether to
  reproduce the mangling is left to Grid parity, which can see it; a unit test cannot.
- The state machine has no JavaScript oracle of its own. Unlike the width port and the Line editor
  it is verified by a close reading and unit tests, and its real check is the harvested `text`
  Scenarios of ADR-0003 — which is the argument for landing those next rather than the widget.
