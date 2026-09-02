// The third Recorder: the static renderers. Runs scripts/static/cases.mjs against clack and writes
// crates/clark-core/tests/fixtures/static.json.
//
//   node scripts/harvest-static.mjs
//
// `log`, `intro`, `outro` and `cancel` are not Prompts — no state, no keys, no second Frame — so
// they cannot be carried by a Scenario, which is a sequence of events. A case here is one call and
// one string of bytes, and the Fixture is flat.
//
// Deliberate, like the other two harvests — never CI.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { TAG, checkout, root } from './upstream.mjs';

const { core, describe, head, prompts } = checkout({ built: true });
const outDir = join(root, 'crates',
	'clark-core', 'tests', 'fixtures');

// --- run ------------------------------------------------------------------------------------

const spool = mkdtempSync(join(tmpdir(), 'clark-static-'));

try {
	execFileSync(
		'npx',
		['vitest', 'run', '--config', join(root, 'scripts', 'static', 'vitest.config.mjs')],
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
	console.error('\na case wrote nothing; nothing was recorded.');
	rmSync(spool, { recursive: true, force: true });
	process.exit(1);
}

const records = readdirSync(spool)
	.filter((f) => f.endsWith('.json'))
	.sort()
	.map((f) => JSON.parse(readFileSync(join(spool, f), 'utf8')));
rmSync(spool, { recursive: true, force: true });

const { cases } = await import('./static/cases.mjs');
if (records.length !== cases.length) {
	console.error(`recorded ${records.length} of ${cases.length} cases; nothing was written.`);
	process.exit(1);
}

// --- write ------------------------------------------------------------------------------------

mkdirSync(outDir, { recursive: true });

const fixture = {
	source: 'scripts/static/cases.mjs',
	tag: TAG,
	commit: head,
	describe,
	node: process.versions.node,
	generatedBy: 'scripts/harvest-static.mjs',
	cases: records,
};

const out = join(outDir, 'static.json');
writeFileSync(out, `${JSON.stringify(fixture, null, '\t')}\n`);

console.log(`\n${records.length} cases -> ${out}`);
