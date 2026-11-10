# 08: Validating state with a blocking slow validator (clack #583)

**What to build:** clack validates asynchronously: on submit with a Promise-returning validator, the Prompt enters a `validating` state, ignores keys, and redraws when the Promise settles. clark keeps its validators synchronous and reproduces the same Grids with an opt-in "slow" validator. On Enter the Prompt moves to Validating and that Frame is emitted. Then the closure runs blocking, any key events that queued meanwhile are discarded, and the Prompt continues to Error or Submit as today. Every Prompt draws Validating with the dim active step symbol; text additionally draws its own frame: the question, then the bar with the dim input, then the end bar with a dim `Validating...`. The existing synchronous validator API is unchanged.

**Blocked by:** 02

**Status:** done

- [x] A `Validating` status exists, and the Theme's step symbol for it is the dim active symbol
- [x] text renders the Validating frame described above, Guided and unGuided
- [x] An opt-in slow validator causes the Validating Frame to be emitted before the closure runs
- [x] Keys pressed while it runs are dropped, not replayed afterwards
- [x] Existing synchronous validators behave exactly as before
- [x] A new ADR records why a blocking call produces the same Grids as clack's Promise
- [x] Any harvested `validating` text Scenarios pass
