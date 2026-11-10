# A slow validator blocks, and the Grid cannot tell

clack 1.6.0's core added a `validating` state (clack #583). When `validate` returns a `Promise`,
`onKeypress` sets `state = 'validating'`, calls `render()`, `await`s, and only then carries on to `error` or
`submit`. While the Promise is pending, `onKeypress` returns on its first line, so every key is
ignored. `text` draws its own Frame for the state (dim user input, `Validating...` on the foot) and every
Prompt's `symbol()` draws a dim `◆`.

clark's validators are synchronous, and they stay that way. An async `Validator` would put an
executor under every Prompt for a feature most callers never use. The port reproduces the state
with an opt-in **slow** validator instead:
[`Prompt::with_slow_validator`](../../crates/clark-core/src/prompt.rs), and `.validate_slow()` on
the `text()` builder.

## How the blocking call produces the same Grid

The bytes clack writes for a Promise validator are: the Validating Frame, then (some time later)
the Error or settled Frame. Nothing in between. The Grid is a function of those bytes alone, so
the time between them is invisible to it.

The port splits `onKeypress` at the same `await`:

1. `Prompt::key` on `return` with a slow validator sets `Status::Validating` and returns. It
   skips the cancel check and `finalize`, which upstream also runs only after the `await`.
   `Session::key` then writes the Validating Frame.
2. The driver writes those bytes, then calls `Session::validate`. That runs the closure (blocking),
   then the cancel check and `finalize`, and composes the Frame it settles on. The driver writes that too.

So the same two Frames are written in the same order, from the same state machine.

## Keys during validation are dropped

Upstream ignores keys while the Promise is pending, but they still arrive, one by one. Here they
queue in the terminal's input while the closure blocks. Replaying them afterwards would type into
a Prompt that clack would have left untouched, so the driver drains the queue once `validate`
returns and discards every key. A resize in that queue is still applied, because upstream calls `render()`
on resize in any state. `Prompt::key` also ignores keys while Validating, so a driver that does not
drain them still matches upstream.

## Notes

- **The Recorder now writes `async: true`** next to `callback: true` when the callback is an
  `AsyncFunction`. That is the only way a Scenario can say which path clack took. The three
  validating `text` Scenarios are all replayed with upstream's `should be xy` validator, the same
  bargain the loader already makes for `required` and `filter`.
- **`symbolBar` has no `validating` arm upstream**, so a `select` or `multiselect` with a Promise
  validator would draw the literal string `undefined` as its Guide. Not reproduced: `Theme::bar`
  gives Validating the active colour.
- **Only `text()` has `.validate_slow()`.** Every widget draws Validating (the dim step, otherwise
  its active Frame, as upstream's `default:` branches do), so adding it to another builder means
  adding the setter and nothing else.
