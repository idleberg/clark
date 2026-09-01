// Stands in for `@clack/prompts`' `src/autocomplete.js` while the Recorder runs, for the one suite
// upstream that reaches past the entry point. See ./prompts-shim.mjs, which this copies; the two
// are separate files because an alias replaces one specifier with one module, and `autocomplete`
// has to be reachable under both the name the other tests import it by and the one this suite does.
//
// `clack:autocomplete-src` is an alias for the real module, defined in ./vitest.config.mjs. It is
// the same file `src/index.ts` re-exports, so a prompt recorded here is the same object either way.
// `settings` comes from the entry point rather than from `@clack/core` directly, for the reason
// ./setup.mjs gives: under pnpm the bare specifier resolves to a different instance from here than
// it does from inside the package, and it is the package's instance the tests are changing.

import * as real from 'clack:autocomplete-src';
import { settings } from 'clack:prompts-src';
import { begin } from './setup.mjs';

export * from 'clack:autocomplete-src';

const watch = (kind) => {
	const fn = real[kind];
	return (opts) => {
		begin(kind, opts, settings);
		return fn(opts);
	};
};

export const autocomplete = watch('autocomplete');
export const autocompleteMultiselect = watch('autocompleteMultiselect');
