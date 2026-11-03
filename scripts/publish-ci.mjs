import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout } from 'node:timers/promises';

const directory = dirname(fileURLToPath(import.meta.url));
const { registry, packages } = JSON.parse(
  readFileSync(resolve(directory, 'release-manifest.json'), 'utf8'),
);
const tag = process.env.NPM_TAG;
const dryRun = process.argv.includes('--dry-run');
const bootstrap = process.argv.includes('--bootstrap');
if (bootstrap && process.env.CI) {
  throw new Error('Bootstrap publishing requires a local authenticated npm session');
}
if (!dryRun && !tag) throw new Error('NPM_TAG is required');
const waitMs = Number(process.env.NPM_PUBLISH_WAIT_MS ?? 1_800_000);
if (!Number.isSafeInteger(waitMs) || waitMs <= 0) throw new Error('Invalid NPM_PUBLISH_WAIT_MS');
if (!Array.isArray(packages) || packages.length === 0) throw new Error('Missing release packages');
const names = new Set();
for (const entry of packages) {
  if (
    names.has(entry.name) ||
    basename(entry.filename) !== entry.filename ||
    !entry.filename.endsWith('.tgz')
  ) {
    throw new Error('Invalid release manifest');
  }
  names.add(entry.name);
  const actual = `sha512-${createHash('sha512')
    .update(readFileSync(resolve(directory, entry.filename)))
    .digest('base64')}`;
  if (actual !== entry.integrity) throw new Error(`Archive integrity mismatch: ${entry.name}`);
}
const dependencies = new Map(
  packages.map((entry) => {
    const manifest = JSON.parse(
      execFileSync('tar', ['-xOf', resolve(directory, entry.filename), 'package/package.json'], {
        encoding: 'utf8',
      }),
    );
    if (manifest.name !== entry.name || manifest.version !== entry.version)
      throw new Error(`Archive metadata mismatch: ${entry.name}`);
    return [
      entry.name,
      Object.keys({ ...manifest.dependencies, ...manifest.optionalDependencies }).filter((name) =>
        names.has(name),
      ),
    ];
  }),
);
const pending = new Set(names);
const waves = [];
while (pending.size > 0) {
  const wave = packages.filter(
    (entry) =>
      pending.has(entry.name) && dependencies.get(entry.name).every((name) => !pending.has(name)),
  );
  if (wave.length === 0) throw new Error('Circular release dependencies');
  waves.push(wave);
  for (const entry of wave) pending.delete(entry.name);
}
function publishedIntegrity(entry) {
  try {
    return JSON.parse(
      execFileSync(
        'npm',
        [
          'view',
          `${entry.name}@${entry.version}`,
          'dist.integrity',
          '--json',
          '--registry',
          registry,
          '--prefer-online',
        ],
        { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 60000 },
      ),
    );
  } catch (error) {
    let result;
    try {
      result = JSON.parse(error.stdout?.toString() ?? '');
    } catch {
      /* Preserve the original error below. */
    }
    if (result?.error?.code === 'E404') return null;
    throw error;
  }
}
const resultsPath = resolve(directory, 'publish-results.json');
const previous = existsSync(resultsPath) ? JSON.parse(readFileSync(resultsPath, 'utf8')) : null;
const results = new Map(
  (previous?.registry === registry ? previous.packages : [])
    .filter((result) =>
      packages.some(
        (entry) =>
          entry.name === result.name &&
          entry.version === result.version &&
          entry.integrity === result.integrity,
      ),
    )
    .map((result) => [result.name, result]),
);
function record(entry, status) {
  results.set(entry.name, { ...entry, tag, status });
  writeFileSync(
    resultsPath,
    JSON.stringify({ registry, packages: [...results.values()] }, null, 2) + '\n',
  );
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(
      process.env.GITHUB_STEP_SUMMARY,
      `- ${entry.name}@${entry.version}: ${status} (tag: ${tag}).\n`,
    );
  }
}
if (!dryRun && process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    '### npm publishing results\n\nPlatform packages are published and verified in the registry before core.\n\n',
  );
}
for (const [index, wave] of waves.entries()) {
  if (dryRun) {
    console.log(
      `Wave ${index + 1}: ${wave.map((entry) => `${entry.name}@${entry.version}`).join(', ')}`,
    );
    continue;
  }
  const awaiting = new Map();
  for (const entry of wave) {
    try {
      const integrity = publishedIntegrity(entry);
      if (integrity && integrity !== entry.integrity)
        throw new Error(`Published content differs: ${entry.name}@${entry.version}`);
      if (integrity) {
        record(entry, 'already-published');
        console.log(`Verified published ${entry.name}@${entry.version}`);
        continue;
      }
      if (results.get(entry.name)?.status === 'submitted') {
        awaiting.set(entry.name, entry);
        console.log(`Waiting for previously submitted ${entry.name}@${entry.version}`);
        continue;
      }
      execFileSync(
        'npm',
        [
          'publish',
          resolve(directory, entry.filename),
          '--ignore-scripts',
          '--access',
          'public',
          bootstrap ? '--provenance=false' : '--provenance',
          '--tag',
          tag,
          '--registry',
          registry,
        ],
        { stdio: 'inherit' },
      );
      record(entry, 'submitted');
      awaiting.set(entry.name, entry);
    } catch (error) {
      record(entry, 'failed');
      throw error;
    }
  }
  const deadline = Date.now() + waitMs;
  while (awaiting.size > 0) {
    for (const entry of awaiting.values()) {
      const published = publishedIntegrity(entry);
      if (published) {
        if (published !== entry.integrity)
          throw new Error(`Published content differs: ${entry.name}@${entry.version}`);
        record(entry, 'published');
        awaiting.delete(entry.name);
        console.log(`Verified published ${entry.name}@${entry.version}`);
      }
    }
    if (awaiting.size === 0) break;
    if (Date.now() >= deadline)
      throw new Error(
        `Registry visibility timeout: ${[...awaiting.keys()].join(', ')}. Preserve publish-results.json and retry after npm scanning completes.`,
      );
    await setTimeout(15_000);
  }
}
