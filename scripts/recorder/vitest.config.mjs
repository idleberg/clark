// Vitest config for the Scenario Recorder. Runs clack's own `@clack/prompts` suite unchanged, with
// two observers attached. Driven by scripts/harvest-scenarios.mjs, which sets the environment.
//
// Nothing is written into the clack checkout. The suite is run from here with `root` pointed at it,
// so a re-harvest at a newer tag is a `git checkout` and nothing else.

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// A plain object rather than `defineConfig`: this file lives outside the clack checkout, so vitest
// loads it from a directory where `vitest/config` cannot be resolved. `defineConfig` is identity.

const here = dirname(fileURLToPath(import.meta.url));

const pkg = process.env.RECORDER_PROMPTS_PKG;
if (!pkg) throw new Error('RECORDER_PROMPTS_PKG is not set; run scripts/harvest-scenarios.mjs');

export default {
	root: pkg,
	resolve: {
		alias: [
			// The real entry point, under a name the tests never use.
			{ find: 'clack:prompts-src', replacement: join(pkg, 'src', 'index.ts') },
			// Anchored, so it only catches the specifier the test files import by.
			{ find: /^\.\.\/src\/index\.js$/, replacement: join(here, 'prompts-shim.mjs') },
		],
	},
	test: {
		// Copied from packages/prompts/vitest.config.ts, which this file replaces. Without the ANSI
		// serializer every snapshot in the suite fails, and a failing suite means the recording is
		// not of clack. scripts/harvest-scenarios.mjs pins that file's hash so the copy cannot go
		// stale unnoticed at a newer tag.
		env: { FORCE_COLOR: '1' },
		snapshotSerializers: ['vitest-ansi-serializer'],

		setupFiles: [join(here, 'setup.mjs')],
		// One process: the recorder writes a file per test, and fanning out buys nothing on a suite
		// this size while making a failure harder to read.
		pool: 'forks',
		poolOptions: { forks: { singleFork: true } },
	},
};
