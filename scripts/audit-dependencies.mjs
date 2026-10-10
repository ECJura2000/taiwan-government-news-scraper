import { spawnSync } from 'node:child_process';
import { readFileSync, cpSync, mkdtempSync, rmSync, existsSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

// RustSec currently duplicates this ID across two packages absent from our lockfile.
// Never omit an advisory for a dependency present in the project.
const initial = spawnSync('cargo', ['audit'], { encoding: 'utf8' });
process.stdout.write(initial.stdout || '');
process.stderr.write(initial.stderr || '');
if (initial.status === 0) process.exit(0);
if (!initial.stderr?.includes('duplicate advisory ID: RUSTSEC-2026-0244')) process.exit(initial.status || 1);
const lock = readFileSync('Cargo.lock', 'utf8');
if (/^name = "gettext-(rs|sys)"$/m.test(lock)) process.exit(1);
const database = join(process.env.CARGO_HOME || join(homedir(), '.cargo'), 'advisory-db');
const rs = join(database, 'crates/gettext-rs/RUSTSEC-2026-0244.md');
const sys = join(database, 'crates/gettext-sys/RUSTSEC-2026-0244.md');
if (![rs, sys].every(existsSync)) process.exit(1);
if (!readFileSync(rs, 'utf8').includes('package = "gettext-rs"') || !readFileSync(sys, 'utf8').includes('package = "gettext-sys"')) process.exit(1);
const temporary = mkdtempSync(join(tmpdir(), 'news-rustsec-'));
try {
  cpSync(database, temporary, { recursive: true });
  rmSync(join(temporary, 'crates/gettext-sys/RUSTSEC-2026-0244.md'));
  console.log('RustSec duplicate workaround: gettext-rs/gettext-sys are absent from Cargo.lock; scanning every applicable advisory with a temporary database copy.');
  const retry = spawnSync('cargo', ['audit', '--no-fetch', '--db', temporary], { stdio: 'inherit' });
  process.exitCode = retry.status || (retry.error ? 1 : 0);
} finally { rmSync(temporary, { recursive: true, force: true }); }
