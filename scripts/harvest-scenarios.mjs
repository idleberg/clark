// The Recorder. Runs clack's own test suite against an instrumented `@clack/prompts` and writes out
// a Scenario and Fixture pair per test case: the options the test passed, the keypresses it sent,
// and the chunks clack wrote back. See ADR-0003 for why upstream's tests are the specification, and
// ADR-0010 for how they are caught.
//
//   node scripts/harvest-scenarios.mjs text
//   node scripts/harvest-scenarios.mjs text confirm
//
// Needs a clack checkout under prior-art/, at the pinned tag, with `@clack/core` built. Refuses
// rather than guesses: a fixture recorded from the wrong commit is worse than no fixture, because
// it fails somewhere unrelated months later. Deliberate, like `mise run drift` — never CI.

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const clack = join(root, 'prior-art', 'clack');
const prompts = join(clack, 'packages', 'prompts');
const outDir = join(root, 'clackatui-core', 'tests', 'fixtures', 'scenarios');

/** The tag README.md and the lockfile name. Bumping this is what tracking clack looks like. */
const TAG = '@clack/prompts@1.7.0';

const suites = process.argv.slice(2);
if (suites.length === 0) {
	console.error('usage: node scripts/harvest-scenarios.mjs <suite>...   (e.g. text)');
	process.exit(2);
}

// --- preconditions --------------------------------------------------------------------------

const git = (...args) => execFileSync('git', args, { cwd: clack, encoding: 'utf8' }).trim();

if (!existsSync(clack)) {
	console.error(`no clack checkout at ${clack}. See ADR-0008: prior-art/ is not committed.`);
	process.exit(1);
}

const head = git('rev-parse', 'HEAD');
const tagged = git('rev-parse', `${TAG}^{commit}`);
if (head !== tagged) {
	console.error(`prior-art/clack is not at ${TAG}.`);
	console.error(`  HEAD is ${git('describe', '--tags')} (${head.slice(0, 8)})`);
	console.error(`  run: git -C prior-art/clack checkout '${TAG}'`);
	process.exit(1);
}

if (!existsSync(join(clack, 'packages', 'core', 'dist', 'index.mjs'))) {
	console.error('@clack/core is not built. run: pnpm --dir prior-art/clack --filter @clack/core build');
	process.exit(1);
}

// scripts/recorder/vitest.config.mjs stands in for this file and copies its two settings, so a
// change to it upstream has to be looked at rather than silently ignored.
const UPSTREAM_VITEST_CONFIG_SHA =
	'1a2fa42aa1d87b82bb86410553e7fb87a2612107b8cac841645f7c5fa3f672b8';

const upstreamConfig = join(prompts, 'vitest.config.ts');
const actual = createHash('sha256').update(readFileSync(upstreamConfig)).digest('hex');
if (actual !== UPSTREAM_VITEST_CONFIG_SHA) {
	console.error(`${upstreamConfig} has changed since the Recorder last copied it.`);
	console.error('  reconcile scripts/recorder/vitest.config.mjs, then update the pinned hash:');
	console.error(`  ${actual}`);
	process.exit(1);
}

for (const suite of suites) {
	if (!existsSync(join(prompts, 'test', `${suite}.test.ts`))) {
		console.error(`no such suite: packages/prompts/test/${suite}.test.ts`);
		process.exit(1);
	}
}

// --- run ------------------------------------------------------------------------------------

const spool = mkdtempSync(join(tmpdir(), 'clackatui-recorder-'));

try {
	execFileSync(
		'npx',
		[
			'vitest',
			'run',
			'--config',
			join(root, 'scripts', 'recorder', 'vitest.config.mjs'),
			...suites.map((s) => `test/${s}.test.ts`),
		],
		{
			cwd: prompts,
			stdio: 'inherit',
			env: {
				...process.env,
				RECORDER_OUT: spool,
				RECORDER_PROMPTS_PKG: prompts,
			},
		}
	);
} catch {
	// A failing suite means the instrumentation changed what clack does, and the recording is not
	// of clack. Upstream's own snapshots passing is the only check we have on that.
	console.error('\nclack’s suite did not pass under instrumentation; nothing was written.');
	rmSync(spool, { recursive: true, force: true });
	process.exit(1);
}

const records = readdirSync(spool)
	.filter((f) => f.endsWith('.json'))
	.map((f) => JSON.parse(readFileSync(join(spool, f), 'utf8')));
rmSync(spool, { recursive: true, force: true });

// --- collate --------------------------------------------------------------------------------

/** `describe.each(['true','false'])` runs every case twice. `CI` only reaches spinner and task-log,
 *  both of which are v2 — so for the Prompts we record the two runs should be identical, and
 *  collapsing them is how that gets checked rather than assumed. */
const ciRun = /\s*\(isCI = (true|false)\)/;

const collapsed = new Map();
for (const record of records) {
	const name = record.test.replace(ciRun, '').replace(/\s*>\s*/g, ' › ').trim();
	const body = JSON.stringify(record.prompts);
	const seen = collapsed.get(name);
	if (!seen) {
		collapsed.set(name, { name, prompts: record.prompts, body, runs: 1 });
	} else if (seen.body === body) {
		seen.runs += 1;
	} else {
		// Keep both under their full names rather than silently picking one.
		collapsed.set(record.test, { name: record.test, prompts: record.prompts, body, runs: 1 });
		seen.diverged = true;
	}
}

const diverged = [...collapsed.values()].filter((s) => s.diverged).map((s) => s.name);

const scenarios = [...collapsed.values()]
	.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
	.map(({ name, prompts: runs }) => ({ name, prompts: runs }));

mkdirSync(outDir, { recursive: true });

const fixture = {
	source: '@clack/prompts test suite',
	tag: TAG,
	commit: head,
	describe: git('describe', '--tags'),
	node: process.versions.node,
	generatedBy: 'scripts/harvest-scenarios.mjs',
	suites,
	scenarios,
};

const out = join(outDir, `${suites.join('-')}.json`);
writeFileSync(out, `${JSON.stringify(fixture, null, '\t')}\n`);

const keypresses = scenarios.reduce(
	(n, s) => n + s.prompts.reduce((m, p) => m + p.keys.length, 0),
	0
);
console.log(
	`\n${scenarios.length} scenarios, ${keypresses} keypresses, from ${records.length} recorded tests -> ${out}`
);
if (diverged.length > 0) {
	console.log(`CI changed the outcome of: ${diverged.join(', ')}`);
}
