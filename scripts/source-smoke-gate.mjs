import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {evaluateGate} from './release-source-gate.mjs';
import {loadApprovedSources, matchesApprovedSources} from './check-source-catalog.mjs';
import {resolveSmokeProfile} from './source-smoke-profile.mjs';

export function evaluateSourceSmoke(report, retries, previous = null, {
  maxFailed = 8, maxFailureRate = 0.10, maxConsecutiveFailures = 3,
  expectedSources = loadApprovedSources(),
} = {}) {
  if (!Number.isInteger(maxConsecutiveFailures) || maxConsecutiveFailures < 1) throw Error('Invalid consecutive failure limit');
  const gate = evaluateGate(report, retries, {maxFailed, maxFailureRate, criticalSources: [], expectedSources});
  const errors = [...gate.errors];
  const selected = Array.isArray(report.selected_sources) ? report.selected_sources : [];
  const initial = Array.isArray(report.failed_sources) ? report.failed_sources : [];
  const diagnostics = report.source_diagnostics;
  if (!Array.isArray(diagnostics) || diagnostics.length !== selected.length ||
      new Set(diagnostics.map(d => d?.source)).size !== selected.length ||
      diagnostics.some(d => !selected.includes(d?.source) || !['success', 'failed'].includes(d.status) ||
        (d.status === 'failed') !== initial.includes(d.source))) errors.push('inconsistent source diagnostics');
  for (const retry of retries) {
    if (!initial.includes(retry.source)) errors.push(`unexpected retry evidence: ${retry.source}`);
    if (retry.selected_source_count !== 1 || !Array.isArray(retry.selected_sources) ||
        retry.selected_sources.length !== 1 || retry.selected_sources[0] !== retry.source ||
        !Array.isArray(retry.quality?.alert_reasons) || retry.quality.alert_reasons.length) {
      errors.push(`invalid source-only retry contract: ${retry.source}`);
    }
  }
  const validHistory = previous === null || (previous?.version === 1 &&
    typeof previous.last_run_id === 'string' && previous.streaks &&
    typeof previous.streaks === 'object' && !Array.isArray(previous.streaks) &&
    Object.entries(previous.streaks).every(([source, count]) => source && Number.isSafeInteger(count) && count >= 0));
  if (!validHistory) errors.push('invalid persisted source-health history');
  const runId = report.run_id ?? report.started_at;
  if (typeof runId !== 'string' || !runId) errors.push('missing run identity');
  const historyUpdateAllowed = validHistory && typeof runId === 'string' && !!runId &&
    report.report_schema_version === 4 && report.selected_source_count === expectedSources.length &&
    matchesApprovedSources(selected, expectedSources) && Array.isArray(report.failed_sources) && new Set(initial).size === initial.length &&
    initial.every(source => selected.includes(source)) && Array.isArray(report.quality?.alert_reasons) &&
    report.status === (initial.length ? "partial_failure" : report.quality.alert_reasons.length ? "attention" : "success") &&
    Number.isSafeInteger(report.news_count) && report.news_count > 0 &&
    /^[0-9a-f]{64}$/.test(report.relevance_policy?.ruleset_hash ?? "") &&
    !errors.some(error => ["diagnostics", "retry contract", "unexpected retry", "missing/duplicate retry"].some(message => error.includes(message)));
  const oldStreaks = validHistory ? previous?.streaks ?? {} : {};
  const streaks = {...oldStreaks};
  const repeat = previous?.last_run_id === runId;
  const unresolved = new Set(gate.final_failed_sources);
  for (const source of historyUpdateAllowed ? selected : []) {
    // A retry recovery closes the streak; unrelated source-only runs do not change it.
    if (!unresolved.has(source)) streaks[source] = 0;
    else if (!repeat) streaks[source] = (oldStreaks[source] ?? 0) + 1;
  }
  const persistent = selected.filter(source => unresolved.has(source) && streaks[source] >= maxConsecutiveFailures);
  for (const source of persistent) errors.push(`source failed ${streaks[source]} consecutive full checks: ${source}`);
  return {...gate, accepted: errors.length === 0, errors,
    history_update_allowed: historyUpdateAllowed,
    consecutive_failure_limit: maxConsecutiveFailures, persistent_failed_sources: persistent,
    history: {version: 1, last_run_id: runId, streaks}};
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [reportPath, retryRoot, historyPath, outputPath, profileName = 'full'] = process.argv.slice(2);
  const profile = resolveSmokeProfile(profileName);
  const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
  const retries = fs.readdirSync(retryRoot).map(dir => path.join(retryRoot, dir, 'retry-evidence.json'))
    .filter(file => fs.existsSync(file)).map(file => JSON.parse(fs.readFileSync(file, 'utf8')));
  const previous = fs.existsSync(historyPath) ? JSON.parse(fs.readFileSync(historyPath, 'utf8')) : null;
  const result = evaluateSourceSmoke(report, retries, previous, {expectedSources:profile.expected_sources});
  result.verification_scope = profile;
  fs.writeFileSync(outputPath, JSON.stringify(result, null, 2) + '\n');
  // Preserve evidence after a failed gate, so a persistent outage cannot reset itself.
  if (result.history_update_allowed) {
    fs.writeFileSync(historyPath + '.tmp', JSON.stringify(result.history, null, 2) + '\n');
    fs.renameSync(historyPath + '.tmp', historyPath);
  }
  console.log(JSON.stringify(result));
  process.exitCode = result.accepted ? 0 : 1;
}
