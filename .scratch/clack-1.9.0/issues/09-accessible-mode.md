# 09: Accessible mode for spinner and progress (clack #593, #595, #596)

**What to build:** An `accessible` setting with clack's resolution order: the per-call option, then the global setting, then the `ACCESSIBLE` environment variable, which counts as on for any non-empty value except `0` and `false`. When on, spinner (and progress, which is a spinner whose message is drawn, ADR-0033) writes static, append-only output for screen readers:
- start writes the message on its own line (nothing for an empty message), with no Guide bar, no animation and no cursor hiding
- every 30 seconds it writes `still working: <message>`, or `still working` with no message
- stop, cancel and error write the bare final message, plus the timer when the indicator is the timer; an empty message falls back to `Done`, the cancel message or the error message
- nothing is erased
clark-cli gets an `--accessible` flag for spinner and progress.

**Blocked by:** 02

**Status:** done

- [x] The setting and the per-call option resolve in the order above, with unit tests for the env-var values `""`, `0`, `false` and `1`
- [x] Accessible spinner and progress write exactly the lines above
- [x] Non-accessible output is unchanged
- [x] The accessible scripted cases recorded in 02 pass
- [x] clark-cli has `--accessible`, documented in its README
- [x] A new ADR records the design

## Notes

- clark-cli has no `progress` sub-command, so `--accessible` is on `spinner` only.
