import {test} from 'node:test';
import assert from 'node:assert/strict';
import {evaluateSourceSmoke} from './source-smoke-gate.mjs';
import {loadApprovedSources} from './check-source-catalog.mjs';
const names = loadApprovedSources();
const report = (failed, runId = 'run-1') => ({run_id: runId, status: failed.length ? 'partial_failure' : 'success',
  report_schema_version: 4, selected_source_count: names.length, selected_sources: names, news_count: 10,
  failed_sources: failed, quality: {alert_reasons: []}, relevance_policy: {ruleset_hash: 'a'.repeat(64)},
  source_diagnostics: names.map(source => ({source, status: failed.includes(source) ? 'failed' : 'success'}))});
const retry = (source, recovered = false) => ({source, selected_source_count: 1, selected_sources: [source],
  quality: {alert_reasons: []}, exit_code: recovered ? 0 : 1, retry_status: recovered ? 'success' : 'partial_failure',
  failed_sources: recovered ? [] : [source], failure_class_counts: recovered ? {} : {runner_network: 1}});
test('daily smoke shares release final count/rate budget', () => {
  const failures = names.slice(0, 9);
  assert.equal(evaluateSourceSmoke(report(failures), failures.map(n => retry(n))).accepted, false);
  assert.equal(evaluateSourceSmoke(report(failures), failures.map((n, i) => retry(n, i === 0))).accepted, true);
});
test('persistent failure is blocked on the third check and recovery resets it', () => {
  const failed = [names[0]], evidence = [retry(names[0])];
  let state = null;
  for (let run = 1; run <= 3; run++) {
    const result = evaluateSourceSmoke(report(failed, `run-${run}`), evidence, state);
    assert.equal(result.accepted, run < 3);
    assert.equal(result.history.streaks[names[0]], run);
    state = result.history;
  }
  const repeated = evaluateSourceSmoke(report(failed, 'run-3'), evidence, state);
  assert.equal(repeated.history.streaks[names[0]], 3);
  const recovered = evaluateSourceSmoke(report(failed, 'run-4'), [retry(names[0], true)], state);
  assert.equal(recovered.accepted, true);
  assert.equal(recovered.history.streaks[names[0]], 0);
});
test('malformed history, diagnostics, retries, and quality fail closed', () => {
  assert.equal(evaluateSourceSmoke(report([]), [], {version: 1, last_run_id: 'x', streaks: []}).accepted, false);
  assert.equal(evaluateSourceSmoke({...report([]), source_diagnostics: []}, []).accepted, false);
  const failed = [names[0]];
  assert.equal(evaluateSourceSmoke(report(failed), [{...retry(names[0], true), selected_sources: [names[1]]}]).accepted, false);
  assert.equal(evaluateSourceSmoke({...report([]), quality: {alert_reasons: ['detail_coverage_below_minimum']}}, []).accepted, false);
  assert.throws(() => evaluateSourceSmoke(report([]), [], null, {maxConsecutiveFailures: 0}));
});

test('invalid reports cannot rewrite a previously valid failure history', () => {
  const previous = {version: 1, last_run_id: 'old', streaks: {[names[0]]: 2}};
  const result = evaluateSourceSmoke({...report([]), source_diagnostics: []}, [], previous);
  assert.equal(result.history_update_allowed, false);
  assert.equal(result.history.streaks[names[0]], 2);
  const missing = evaluateSourceSmoke(report([names[0]]), [], previous);
  assert.equal(missing.history_update_allowed, false);
  assert.equal(missing.history.streaks[names[0]], 2);
  const changed=[...names];changed[0]='unapproved-source';
  const mismatch=evaluateSourceSmoke({...report([]),selected_sources:changed},[],previous);
  assert.equal(mismatch.accepted,false);
  assert.equal(mismatch.history_update_allowed,false);
  assert.equal(mismatch.history.streaks[names[0]],2);
});

test('command stores failure history even on gate rejection and preserves it for invalid evidence', async () => {
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const {spawnSync} = await import('node:child_process');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'source-smoke-gate-'));
  try {
    const reports = path.join(root, 'report.json'), retryRoot = path.join(root, 'retries');
    const history = path.join(root, 'history.json'), output = path.join(root, 'gate.json');
    fs.mkdirSync(path.join(retryRoot, 'one'), {recursive: true});
    fs.writeFileSync(path.join(retryRoot, 'one', 'retry-evidence.json'), JSON.stringify(retry(names[0])));
    for (let run = 1; run <= 3; run++) {
      fs.writeFileSync(reports, JSON.stringify(report([names[0]], `cli-${run}`)));
      const result = spawnSync(process.execPath, ['scripts/source-smoke-gate.mjs', reports, retryRoot, history, output]);
      assert.equal(result.status, run < 3 ? 0 : 1, result.stderr.toString());
      assert.equal(JSON.parse(fs.readFileSync(history)).streaks[names[0]], run);
    }
    const saved = fs.readFileSync(history, 'utf8');
    fs.writeFileSync(reports, JSON.stringify({...report([names[0]], 'invalid'), source_diagnostics: []}));
    const result = spawnSync(process.execPath, ['scripts/source-smoke-gate.mjs', reports, retryRoot, history, output]);
    assert.equal(result.status, 1);
    assert.equal(fs.readFileSync(history, 'utf8'), saved);
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});
