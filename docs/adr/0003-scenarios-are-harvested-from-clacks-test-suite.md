# Scenarios are harvested from clack's own test suite

clack's ~500 test cases already have exactly the shape a Scenario needs — a Prompt configuration, a
sequence of `input.emit('keypress', …)` calls, and a captured output buffer. The Recorder therefore
clones clack at tag `@clack/prompts@1.7.0`, patches `MockReadable`/`MockWritable` to log key events
and output chunks, and runs their vitest suite to emit Scenario and Fixture pairs. Upstream's
accumulated regression history becomes the specification, and re-harvesting at a newer tag is how we
track clack over time.

## Consequences

Harvesting inherits upstream's blind spots along with its coverage: their tests fix the terminal at
80x20, never send a resize, and use ASCII input only — precisely the conditions under which a
Buffer-based renderer is most likely to agree with a string-based one by accident. A hand-authored
set covering narrow and wide terminals, mid-Prompt resize, CJK and emoji input, and long values is
therefore part of the suite, not an optional extra.

Scenarios specify semantic key events rather than raw bytes. Cutting below Node's `readline` parser
keeps Prompt Scenarios about Prompt behaviour; disagreements between `readline` and crossterm's
parsers are isolated in their own Conformance suite, where a failure reads as "the parsers differ"
instead of failing several hundred Scenarios for an invisible reason.
