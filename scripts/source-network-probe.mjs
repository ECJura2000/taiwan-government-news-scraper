import fs from 'node:fs';
import path from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';

// Diagnostic evidence only. No TLS bypass, credentials, redirects or acceptance changes.
const run = promisify(execFile);
const catalog = JSON.parse(fs.readFileSync('src-tauri/resources/sources.json', 'utf8'));
const output = process.argv[2];
if (!output) throw Error('Usage: source-network-probe.mjs OUTPUT_JSON');
const targets = new Map();
for (const source of catalog) {
  for (const route of source.routes ?? []) {
    const url = new URL(route.url);
    if (url.protocol !== 'https:' || url.username || url.password) continue;
    // One public TLS endpoint per host; never fetch full articles or personal data.
    targets.set(url.hostname, `https://${url.hostname}/`);
  }
}
const result = [];
const pending = [...targets];
await Promise.all(Array.from({length: 8}, async () => {
  while (pending.length) {
    const [host, url] = pending.shift();
    const args = ['--silent', '--show-error', '--head', '--output', '/dev/null',
      '--connect-timeout', '8', '--max-time', '12', '--write-out', '%{json}', url];
    let stdout, stderr, exitCode;
    try {
      const response = await run('curl', args, {timeout: 15000, maxBuffer: 65536});
      ({stdout, stderr} = response); exitCode = 0;
    } catch (error) {
      ({stdout, stderr} = error); exitCode = error.code;
    }
    let metrics = {};
    try { metrics = JSON.parse(stdout); } catch { /* Preserve stderr and exit code. */ }
    result.push({host, exit_code: exitCode, error: stderr?.trim() ?? '',
      http_status: metrics.http_code ?? null, remote_ip: metrics.remote_ip ?? null,
      tls_verify_result: metrics.ssl_verify_result ?? null,
      dns_seconds: metrics.time_namelookup ?? null, connect_seconds: metrics.time_connect ?? null,
      tls_seconds: metrics.time_appconnect ?? null, total_seconds: metrics.time_total ?? null});
  }
}));
const evidence = {version: 1, diagnostic_only: true, tls_verification_enabled: true,
  checked_at: new Date().toISOString(), probes: result.sort((a,b)=>a.host.localeCompare(b.host))};
fs.mkdirSync(path.dirname(output), {recursive: true});
fs.writeFileSync(output, JSON.stringify(evidence, null, 2) + '\n');
console.log(JSON.stringify(evidence));
