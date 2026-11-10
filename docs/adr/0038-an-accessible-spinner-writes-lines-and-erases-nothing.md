# An accessible spinner writes lines and erases nothing

clack 1.9.0 added an `accessible` mode (clack #593, #595, #596). For a screen reader, a row that is
redrawn ten times a second is noise, so the spinner stops drawing and writes plain lines instead:
the message on start, `still working: <message>` every 30 seconds, and the bare final message on
stop, cancel or error. Nothing is erased and the cursor is never hidden.

## Resolution

Upstream resolves it with `isAccessible(option)`: the per-call option, then the global
`settings.accessible`, then the `ACCESSIBLE` environment variable, which is on for any non-empty
value except `0` and `false`. clark has no process-global settings (see `settings.rs`), so the
"global" is a [`Settings`](../../crates/clark-core/src/settings.rs) value, as `with_guide` already
is. `Settings::accessible` is an `Option<bool>` because `None` and `Some(false)` mean different
things: only `None` lets the environment decide.

The spinner and progress builders take `.accessible(bool)` and `.settings(Settings)`, and resolve
once, at `start`. The core [`spinner::Options`](../../crates/clark-core/src/spinner.rs) carries the
resolved `bool`, so the spinner itself never reads the environment; only `Settings::is_accessible`
does, as `Theme::detect` already does for Unicode.

## One branch in the spinner, none in the progress bar

ADR-0033 makes a progress bar a spinner whose message is drawn, and that pays off here: the bar is
the message, so the accessible start line and heartbeat carry the bar and its colours, exactly as
upstream's do, and clark-core's `progress.rs` did not change. Its driver only gained the same
two builder setters as the spinner's.

Accessible mode takes precedence over `ci`, `with_guide`, the frames and `delay`. Upstream ignores
`delay` because the heartbeat is its only interval, so the ticker runs at
[`spinner::HEARTBEAT`](../../crates/clark-core/src/spinner.rs) and the core treats every tick as a
heartbeat. That is also how the Recorder's scripted cases are written (`delay: 30_000`, one tick per
heartbeat).

## The ticker had to learn to wake up

The ticker thread used to `sleep(delay)`, and ending joined it. With an 80ms frame that is
invisible; with a 30-second heartbeat, `stop()` would hang for up to 30 seconds. The thread now
parks until its deadline and an ending unparks it.

## Notes

- **The empty-message fallbacks** are `Done` for stop, and `settings.messages.cancel` and
  `.error` for cancel and error. Upstream reads its own `cancelMessage`/`errorMessage` options first;
  clark's spinner does not port those (they belong to `handleExit`, which is not ported either),
  so `Settings::messages` is the only override.
- **`clear` writes nothing.** Upstream's is a silent stop, and in accessible mode there is no row
  to erase and no cursor to show.
- **clark-cli's `--accessible`** is on `spinner` only, since the CLI has no `progress` sub-command.
  `--accessible` or `--accessible true` turns it on, `--accessible false` turns it off over
  `ACCESSIBLE`, and leaving it out defers to `ACCESSIBLE`. `--delay` is ignored while accessible.
