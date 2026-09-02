// Stands in for `@clack/prompts`' own entry point while the Recorder runs. Every prompt function is
// wrapped so that the options a test passes are written down along with the keypresses it sends and
// the output it gets back. See ADR-0010.
//
// This is the level the recording has to happen at. `text({ message })` closes over `message` in the
// `render` callback it builds and hands the core Prompt only the rest, so a hook further down never
// sees the question being asked. It is also the last place the test's `input` and `output` streams
// are identifiable as objects rather than as whichever `@clack/core` instance a module resolver
// happened to pick.
//
// `clack:prompts-src` is an alias for the real entry point, defined in ./vitest.config.mjs. The
// alias that puts this file in its place is anchored to the specifier the tests use
// (`../src/index.js`) and so does not match the bare one used here, which is what keeps this module
// from importing itself.

import * as real from 'clack:prompts-src';
import { begin } from './setup.mjs';

export * from 'clack:prompts-src';

/** A prompt function, recorded. An explicit export shadows the same name from `export *`. */
const watch = (kind) => {
	const fn = real[kind];
	return (opts) => {
		// `real.settings` is `@clack/core`'s, reached through the same import the source uses.
		begin(kind, opts, real.settings);
		return fn(opts);
	};
};

export const text = watch('text');
export const password = watch('password');
export const confirm = watch('confirm');
export const select = watch('select');
export const multiselect = watch('multiselect');
export const selectKey = watch('selectKey');
export const groupMultiselect = watch('groupMultiselect');
export const autocomplete = watch('autocomplete');
export const autocompleteMultiselect = watch('autocompleteMultiselect');
export const date = watch('date');
// `path` is an `autocomplete` underneath, but the wrapping happens here rather than there: what
// `path()` calls is its own import of the real module, not this file's export, so a run is recorded
// once and under the name the test asked for.
export const path = watch('path');
export const multiline = watch('multiline');
