import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export function evaluateGate(report, retries, {
  maxFailed = 8, maxFailureRate = 0.10,
  criticalSources = ['數位發展部', '國發會', '國科會', '經濟部'],
} = {}) {
  const errors = [];
  if (!Number.isInteger(maxFailed) || maxFailed < 0 || !Number.isFinite(maxFailureRate) || maxFailureRate < 0 || maxFailureRate > 1) throw Error('Invalid release gate limits');
  const selected = Array.isArray(report.selected_sources) ? report.selected_sources : [];
  if (report.report_schema_version !== 4 || report.selected_source_count !== 88 || selected.length !== 88 || new Set(selected).size !== 88 || !(report.news_count > 0) || !/^[0-9a-f]{64}$/.test(report.relevance_policy?.ruleset_hash ?? '') || !Array.isArray(report.quality?.alert_reasons) || report.quality.alert_reasons.length) errors.push('invalid full-run contract or quality alerts');
  const failures = Array.isArray(report.failed_sources) ? report.failed_sources : [];
  if (!Array.isArray(report.failed_sources) || new Set(failures).size !== failures.length || failures.some(source => !selected.includes(source))) errors.push('invalid failed source evidence');
  if (report.status !== (failures.length ? 'partial_failure' : 'success')) errors.push('status does not match final source evidence');
  const finalFailures = [];
  for (const source of failures) {
    const matching = retries.filter(r => r.source === source);
    const evidence = matching[0];
    if (matching.length !== 1) { errors.push(`missing/duplicate retry evidence: ${source}`); finalFailures.push(source); continue; }
    if (evidence.retry_status === 'success' && Array.isArray(evidence.failed_sources) && evidence.failed_sources.length === 0 && evidence.exit_code === 0) continue;
    const classes = Object.entries(evidence.failure_class_counts ?? {});
    if (evidence.retry_status !== 'partial_failure' || evidence.exit_code !== 1 || !Array.isArray(evidence.failed_sources) || evidence.failed_sources.length !== 1 || evidence.failed_sources[0] !== source || classes.length === 0 || classes.some(([c, count]) => !['source_outage', 'runner_network', 'tls_certificate'].includes(c) || !Number.isInteger(count) || count < 1)) errors.push(`unresolved parser/access/runtime failure: ${source}`);
    finalFailures.push(source);
  }
  if (finalFailures.length > maxFailed || finalFailures.length / 88 > maxFailureRate) errors.push('final source failures exceed count/rate limit');
  for (const source of criticalSources) {
    if (!selected.includes(source)) errors.push(`missing critical source: ${source}`);
    if (finalFailures.includes(source)) errors.push(`critical source still failed: ${source}`);
  }
  return { accepted: errors.length === 0, errors, initial_failed_sources: failures, final_failed_sources: finalFailures, limits: { maxFailed, maxFailureRate, criticalSources } };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [reportPath, retryRoot, outputPath] = process.argv.slice(2);
  const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
  const evidence = fs.readdirSync(retryRoot).map(dir => path.join(retryRoot, dir, 'retry-evidence.json')).filter(file => fs.existsSync(file)).map(file => JSON.parse(fs.readFileSync(file, 'utf8')));
  const result = evaluateGate(report, evidence);
  fs.writeFileSync(outputPath, JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
  process.exitCode = result.accepted ? 0 : 1;
}
