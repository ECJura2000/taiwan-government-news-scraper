import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {checkSourceCatalog} from './check-source-catalog.mjs';

// A foreground, one-job runner for a reviewed main-branch release only.
// Registration credentials never enter files or diagnostic output.
const repository = 'ECJura2000/taiwan-government-news-scraper';
const runId = process.argv[2];
if (!/^\d+$/.test(runId ?? '') || os.platform() !== 'darwin') {
  throw Error('Usage on macOS: node scripts/start-release-source-runner.mjs <queued-release-run-id>');
}
const api = (endpoint, ...options) => JSON.parse(execFileSync('gh', [
  'api', `repos/${repository}/${endpoint}`, ...options,
], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }));
const run = api(`actions/runs/${runId}`);
const localCommit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (run.event !== 'workflow_dispatch' || run.head_branch !== 'main'
    || run.path !== '.github/workflows/build-release.yml' || run.head_sha !== localCommit
    || run.status === 'completed') {
  throw Error('Runner requires an active manual main release at the exact reviewed local commit.');
}
const queuedGate = api(`actions/runs/${runId}/jobs?per_page=100`).jobs.find(job =>
  job.name === 'Gate all approved live sources' && job.status === 'queued'
  && ['self-hosted', 'macOS', 'news-source-gate'].every(label => job.labels.includes(label)));
if (!queuedGate) throw Error('The release has no queued self-hosted approved-source gate.');
checkSourceCatalog();
const chrome = process.env.NEWS_SCRAPER_CHROME_BIN
  ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
fs.accessSync(chrome, fs.constants.X_OK);
for (const command of ['curl', 'jq', 'tar']) {
  execFileSync('/bin/sh', ['-c', 'command -v "$1"', 'preflight', command], { stdio: 'ignore' });
}
// Inspect existing Rust without repairing or replacing the user's tool binaries.
// The job bootstraps its own rustup/cargo under RUNNER_TEMP before toolchain/cache actions.
let existingCargo;
try {
  const cargo = execFileSync('/bin/sh', ['-c', 'command -v cargo'], {encoding:'utf8'}).trim();
  execFileSync(cargo, ['--version'], {stdio:'ignore'});
  existingCargo = cargo;
} catch {
  const toolchains = path.join(process.env.RUSTUP_HOME ?? path.join(os.homedir(), '.rustup'), 'toolchains');
  for (const name of fs.existsSync(toolchains) ? fs.readdirSync(toolchains).sort() : []) {
    const cargo = path.join(toolchains, name, 'bin', 'cargo');
    try {
      fs.accessSync(cargo, fs.constants.X_OK);
      execFileSync(cargo, ['--version'], {stdio:'ignore'});
      existingCargo = cargo;
      break;
    } catch { /* Continue only through the user's existing installed toolchains. */ }
  }
}
if (!existingCargo) throw Error('No usable existing Cargo toolchain found; runner preflight did not modify local Rust.');
const architecture = os.arch() === 'arm64' ? 'arm64' : os.arch() === 'x64' ? 'x64' : null;
const distribution = api('actions/runners/downloads').find(item =>
  item.os === 'osx' && item.architecture === architecture);
if (!distribution || !/^[a-f0-9]{64}$/i.test(distribution.sha256_checksum ?? '')
    || !distribution.download_url.startsWith('https://github.com/actions/runner/releases/download/')) {
  throw Error('GitHub did not provide an official macOS runner with a SHA-256 checksum.');
}
const parent = path.join(os.homedir(), '.local', 'share', 'taiwan-news-release-runners');
fs.mkdirSync(parent, { recursive: true, mode: 0o700 });
const directory = fs.mkdtempSync(path.join(parent, `${runId}-`));
console.log(`Preparing one-job release runner for ${runId} at commit ${localCommit}.`);
const response = await fetch(distribution.download_url);
if (!response.ok) throw Error(`Runner download failed: HTTP ${response.status}`);
const archive = Buffer.from(await response.arrayBuffer());
if (createHash('sha256').update(archive).digest('hex') !== distribution.sha256_checksum.toLowerCase()) {
  throw Error('Official runner checksum mismatch.');
}
const archivePath = path.join(directory, distribution.filename);
fs.writeFileSync(archivePath, archive, { mode: 0o600 });
execFileSync('tar', ['-xzf', archivePath, '-C', directory], { stdio: 'ignore' });
fs.unlinkSync(archivePath);
const name = `news-source-gate-${runId}-${path.basename(directory).split('-').at(-1)}`;
let registered = false;
try {
  const { token } = api('actions/runners/registration-token', '-X', 'POST');
  try {
    execFileSync(path.join(directory, 'config.sh'), [
      '--unattended', '--ephemeral', '--url', `https://github.com/${repository}`,
      '--token', token, '--name', name, '--labels', 'news-source-gate', '--work', '_work',
    ], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] });
  } catch {
    throw Error('Runner registration failed; authentication output was suppressed.');
  }
  registered = true;
  console.log('Starting the queued source gate. This runner exits after one job; no service is installed.');
  const exitCode = await new Promise((resolve, reject) => {
    const child = spawn(path.join(directory, 'run.sh'), [], {
      cwd: directory, stdio: 'inherit',
      env: { ...process.env, NEWS_SCRAPER_CHROME_BIN: chrome, NEWS_SOURCE_EXISTING_CARGO: existingCargo },
    });
    const interrupt = () => child.kill('SIGINT');
    process.once('SIGINT', interrupt);
    process.once('SIGTERM', interrupt);
    child.once('error', reject);
    child.once('exit', code => {
      process.removeListener('SIGINT', interrupt);
      process.removeListener('SIGTERM', interrupt);
      resolve(code ?? 1);
    });
  });
  process.exitCode = exitCode;
} finally {
  if (registered) {
    // Ephemeral runners normally unregister themselves. Remove only this name
    // if interruption left registration behind; never touch another runner.
    const ownRunner = api('actions/runners?per_page=100').runners.find(item => item.name === name);
    if (ownRunner) execFileSync('gh', ['api', '-X', 'DELETE',
      `repos/${repository}/actions/runners/${ownRunner.id}`], { stdio: 'ignore' });
  }
  console.log(`Runner stopped. Local diagnostics remain outside the repository: ${directory}`);
}
