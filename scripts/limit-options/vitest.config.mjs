// Vitest config for the `limitOptions` harvest. Driven by scripts/harvest-limit-options.mjs.
//
// The same arrangement scripts/authored/vitest.config.mjs uses and for the same reason: clack's
// `packages/prompts` is TypeScript and is not built, so the source is reached by alias and Vite
// transpiles it. Nothing is written into the checkout.
//
// Unlike the authored Recorder this drives no Prompt and touches no stream, so there is no
// `process.stdout` to take turns with and no reason to pin the pool.

import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

const pkg = process.env.RECORDER_PROMPTS_PKG;
if (!pkg) throw new Error('RECORDER_PROMPTS_PKG is not set; run scripts/harvest-limit-options.mjs');
const core = process.env.RECORDER_CORE_PKG;
if (!core) throw new Error('RECORDER_CORE_PKG is not set; run scripts/harvest-limit-options.mjs');

export default {
	root: here,
	resolve: {
		alias: [
			{ find: 'clack:limit-options', replacement: join(pkg, 'src', 'limit-options.ts') },
			// `packages/prompts/src` imports `@clack/core` by name; outside the workspace there is
			// no node_modules link to follow, so it is pointed at the source directly.
			{ find: '@clack/core', replacement: join(core, 'src', 'index.ts') },
		],
	},
	test: {
		include: ['record.test.mjs'],
		// picocolors is not in play here — the overflow row is `styleText('dim', '...')` from
		// node:util, which honours FORCE_COLOR the same way. Without this the two ellipsis rows
		// would be indistinguishable from an option whose label happens to be three dots.
		env: { FORCE_COLOR: '1' },
	},
};
