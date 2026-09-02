// Harvests clackatui-core/tests/fixtures/limit-options.json from clack's own `limitOptions`.
//
// Run it from the repository root:
//
//     node scripts/harvest-limit-options.mjs
//
// `limitOptions` decides how a list of options is cut down to what fits, and every list Prompt in
// M3 and M4 is drawn through it. It is a pure function, so it gets a corpus rather than a recording
// — the same arrangement the width, wrap and Emitter ports have (ADR-0008), and for the same
// reason: upstream/ is not committed, so CI has no JavaScript to compare with.
//
// Unlike those three it cannot be reached from a built package. `@clack/prompts` is TypeScript and
// the checkout does not build it, so ./limit-options/vitest.config.mjs aliases the source and Vite
// transpiles it, exactly as the hand-authored Recorder already does for `packages/prompts/src`.
// Vitest is here to compile and to fail loudly, not to test anything: a case that returns a line
// the corpus cannot carry fails, and nothing is written.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { TAG, checkout, root } from './upstream.mjs';

const { core, describe, head, prompts } = checkout({ built: true });
const out = join(root, 'clackatui-core', 'tests', 'fixtures', 'limit-options.json');

// --- run ------------------------------------------------------------------------------------

const spool = mkdtempSync(join(tmpdir(), 'clackatui-limit-options-'));

try {
	execFileSync(
		'npx',
		['vitest', 'run', '--config', join(root, 'scripts', 'limit-options', 'vitest.config.mjs')],
		{
			cwd: prompts,
			stdio: 'inherit',
			env: {
				...process.env,
				RECORDER_OUT: spool,
				RECORDER_PROMPTS_PKG: prompts,
				RECORDER_CORE_PKG: core,
			},
		}
	);
} catch {
	console.error('\na case did not come back in the shape the corpus records; nothing was written.');
	rmSync(spool, { recursive: true, force: true });
	process.exit(1);
}

const records = readdirSync(spool)
	.filter((f) => f.endsWith('.json'))
	.map((f) => JSON.parse(readFileSync(join(spool, f), 'utf8')));
rmSync(spool, { recursive: true, force: true });

const { cases } = await import('./limit-options/cases.mjs');
if (records.length !== cases.length) {
	// Two case names that differ only in punctuation collide as filenames, which would otherwise
	// look like a fixture that is simply a little smaller than the corpus.
	console.error(`recorded ${records.length} of ${cases.length} cases; nothing was written.`);
	process.exit(1);
}

// --- write ------------------------------------------------------------------------------------

const ordered = new Map(records.map((record) => [record.name, record]));
const harvested = cases.map(({ name }) => ordered.get(name));

const fixture = {
	source: 'scripts/limit-options/cases.mjs',
	tag: TAG,
	commit: head,
	describe,
	node: process.versions.node,
	generatedBy: 'scripts/harvest-limit-options.mjs',
	cases: harvested,
};

writeFileSync(out, `${JSON.stringify(fixture, null, '\t')}\n`);

const lines = harvested.reduce((n, c) => n + c.lines.length, 0);
console.log(`\n${harvested.length} cases, ${lines} lines -> ${out}`);
