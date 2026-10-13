import assert from 'node:assert/strict';
import {test} from 'node:test';
import fs from 'node:fs';
import {resolveSmokeProfile} from './source-smoke-profile.mjs';
import {loadApprovedSources} from './check-source-catalog.mjs';
import {evaluateSourceSmoke} from './source-smoke-gate.mjs';
import {buildSmokeSummary} from './source-smoke-summary.mjs';

const approved = loadApprovedSources();
const evidence = JSON.parse(fs.readFileSync('src-tauri/resources/source_verification_profiles.json'));
const options = {approved, evidence, today:evidence.manual_mac_sources.map(row=>row.checked_at).sort().at(-1)};
test('cloud and manual verification exactly partition the approved catalog', () => {
  const cloud = resolveSmokeProfile('cloud', options);
  const full = resolveSmokeProfile('full', options);
  assert.equal(cloud.expected_sources.length + cloud.unverified_sources.length, approved.length);
  assert.deepEqual([...cloud.expected_sources, ...cloud.unverified_sources.map(r=>r.source)].sort(), [...approved].sort());
  assert.deepEqual(full.expected_sources, approved);
  assert.deepEqual(full.unverified_sources, []);
});
test('unknown, duplicate, stale and unproven exclusions fail closed', () => {
  assert.throws(()=>resolveSmokeProfile('other', options));
  assert.throws(()=>resolveSmokeProfile('cloud', {...options,today:evidence.review_after}));
  for (const change of [
    data=>data.manual_mac_sources.push(data.manual_mac_sources[0]),
    data=>data.manual_mac_sources[0].source='未知來源',
    data=>data.manual_mac_sources[0].source='國科會',
    data=>data.manual_mac_sources[0].mac_status='failed',
    data=>data.manual_mac_sources[0].network_probe_run='',
    data=>data.review_after='2026-02-31',
    data=>data.review_after='2099-01-01',
  ]) {
    const copy = structuredClone(evidence); change(copy);
    assert.throws(()=>resolveSmokeProfile('cloud',{...options,evidence:copy}));
  }
  assert.deepEqual(resolveSmokeProfile('full',{...options,today:evidence.review_after}).expected_sources,approved);
});
test('a cloud green gate still cannot claim full or Mac release verification', () => {
  const profile = resolveSmokeProfile('cloud',options);
  const selected = profile.expected_sources;
  const report = {run_id:'profile-fixture',report_schema_version:4,selected_source_count:selected.length,
    selected_sources:selected,news_count:1,status:'success',failed_sources:[],
    relevance_policy:{ruleset_hash:'a'.repeat(64)},quality:{alert_reasons:[]},
    source_diagnostics:selected.map(source=>({source,status:'success'}))};
  const gate = evaluateSourceSmoke(report,[],null,{expectedSources:selected});
  gate.verification_scope=profile;
  assert.equal(gate.accepted,true);
  const summary=buildSmokeSummary(report,gate,[],{environment:'github-hosted',expectedSources:approved});
  assert.equal(summary.complete_source_verification,false);
  assert.equal(summary.not_collected_in_cloud.length,profile.unverified_sources.length);
  assert.equal(summary.release_verification.status,'not_evaluated');
  assert.equal(evaluateSourceSmoke(report,[],null,{expectedSources:approved}).accepted,false);
});
