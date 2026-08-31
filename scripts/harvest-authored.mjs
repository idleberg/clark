// The second Recorder: the hand-authored Scenarios. Runs scripts/authored/cases.mjs against clack
// and writes clackatui-core/tests/fixtures/scenarios/authored.json in the shape the first Recorder
// produces, so the Rust side reads one kind of thing.
//
//   node scripts/harvest-authored.mjs
//
// Why a second one at all: upstream's tests never vary the terminal. Every harvested Scenario is 80
// columns wide and none of them resizes, so wrapping and re-layout — the two places `session.rs`
// records a divergence — reach the Grid comparison untested. A harvest cannot supply what upstream
// never does, so these are written rather than found.
//
// What that costs is the oracle. The first Recorder proves it recorded clack behaving normally by
// leaving upstream's own snapshots passing; there is nothing to pass here. See ADR-0016 and the
// header of scripts/authored/record.test.mjs for what stands in its place. Deliberate, like
// `mise run drift` — never CI.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const clack = join(root, 'prior-art', 'clack');
const prompts = join(clack, 'packages', 'prompts');
const core = join(clack, 'packages', 'core');
const outDir = join(root, 'clackatui-core', 'tests', 'fixtures', 'scenarios');

/** The same tag the harvested Fixture carries. Two recordings of two different clacks would be
 *  compared side by side and nothing would say so. */
const TAG = '@clack/prompts@1.7.0';

// --- preconditions --------------------------------------------------------------------------

if (!existsSync(clack)) {
	console.error(`no clack checkout at ${clack}. See ADR-0008: prior-art/ is not committed.`);
	process.exit(1);
}

const git = (...args) => execFileSync('git', args, { cwd: clack, encoding: 'utf8' }).trim();

const head = git('rev-parse', 'HEAD');
const tagged = git('rev-parse', `${TAG}^{commit}`);
if (head !== tagged) {
	console.error(`prior-art/clack is not at ${TAG}.`);
	console.error(`  HEAD is ${git('describe', '--tags')} (${head.slice(0, 8)})`);
	console.error(`  run: git -C prior-art/clack checkout '${TAG}'`);
	process.exit(1);
}

// --- run ------------------------------------------------------------------------------------

const spool = mkdtempSync(join(tmpdir(), 'clackatui-authored-'));

try {
	execFileSync(
		'npx',
		['vitest', 'run', '--config', join(root, 'scripts', 'authored', 'vitest.config.mjs')],
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
	// A case that did not end where it said it would was not recording what it claims to.
	console.error('\na case did not reach the value it declares; nothing was written.');
	rmSync(spool, { recursive: true, force: true });
	process.exit(1);
}

const records = readdirSync(spool)
	.filter((f) => f.endsWith('.json'))
	.map((f) => JSON.parse(readFileSync(join(spool, f), 'utf8')));
rmSync(spool, { recursive: true, force: true });

const { cases } = await import('./authored/cases.mjs');
if (records.length !== cases.length) {
	console.error(`recorded ${records.length} of ${cases.length} cases; nothing was written.`);
	process.exit(1);
}

// --- write ------------------------------------------------------------------------------------

const scenarios = records.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

mkdirSync(outDir, { recursive: true });

const fixture = {
	source: 'scripts/authored/cases.mjs',
	tag: TAG,
	commit: head,
	describe: git('describe', '--tags'),
	node: process.versions.node,
	generatedBy: 'scripts/harvest-authored.mjs',
	scenarios,
};

const out = join(outDir, 'authored.json');
writeFileSync(out, `${JSON.stringify(fixture, null, '\t')}\n`);

const keypresses = scenarios.reduce(
	(n, s) => n + s.prompts.reduce((m, p) => m + p.keys.length, 0),
	0
);
console.log(`\n${scenarios.length} scenarios, ${keypresses} keypresses -> ${out}`);
