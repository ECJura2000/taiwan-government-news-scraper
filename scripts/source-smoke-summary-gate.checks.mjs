import {test} from 'node:test';
import assert from 'node:assert/strict';
import {buildSmokeSummary, renderSmokeSummary} from './source-smoke-summary.mjs';
import {evaluateSourceSmoke} from './source-smoke-gate.mjs';
import {loadApprovedSources} from './check-source-catalog.mjs';

const sources = loadApprovedSources();
const report = failures => ({run_id:'cloud-test', report_schema_version:4, selected_sources:sources,
  selected_source_count:sources.length, status:failures.length?'partial_failure':'success', news_count:10,
  failed_sources:failures, quality:{alert_reasons:[]}, relevance_policy:{ruleset_hash:'a'.repeat(64)},
  source_diagnostics:sources.map(source=>({source,status:failures.includes(source)?'failed':'success',
    failure_class:source==='文化部'?'access_blocked':'runner_network'}))});
const retry = (source, classification) => ({source,selected_sources:[source],selected_source_count:1,
  quality:{alert_reasons:[]},exit_code:1,retry_status:'partial_failure',failed_sources:[source],
  failure_class_counts:{[classification]:1}});

test('culture cloud limitation never accepts an access-blocked gate', () => {
  const raw=report(['文化部']), retries=[retry('文化部','access_blocked')];
  const gate=evaluateSourceSmoke(raw,retries), before=structuredClone(gate);
  const summary=buildSmokeSummary(raw,gate,retries,{environment:'github-hosted'});
  assert.equal(summary.gate_accepted,false);
  assert.equal(summary.complete_source_verification,false);
  assert.equal(summary.unresolved_failures[0].known_environment_limitation,'official_verification_page_blocks_hosted_runner');
  assert.deepEqual(summary.gate_errors,gate.errors);
  assert.deepEqual(gate,before);
  assert.equal(summary.release_verification.status,'not_evaluated');
  assert.equal(summary.release_verification.paired_evidence,null);
  assert.match(renderSmokeSummary(summary),/完整來源仍未驗證/);
});
test('network failures remain failures without permanent cloud exclusions', () => {
  const failed=sources.filter(source=>source!=='文化部').slice(0,9), raw=report(failed);
  const retries=failed.map(source=>retry(source,'runner_network'));
  const gate=evaluateSourceSmoke(raw,retries);
  const summary=buildSmokeSummary(raw,gate,retries,{environment:'github-hosted'});
  assert.equal(summary.unresolved_failures.length,9);
  assert.equal(summary.gate_accepted,false);
  assert.ok(summary.gate_errors.includes('final source failures exceed count/rate limit'));
  assert.ok(summary.unresolved_failures.every(row=>row.known_environment_limitation===null));
  assert.equal(summary.approved_source_count,sources.length);
});
test('allowed partial outage is not a fully verified catalog or release check', () => {
  const failed=[sources[0]],raw=report(failed),retries=[retry(sources[0],'source_outage')];
  const gate=evaluateSourceSmoke(raw,retries);
  assert.equal(gate.accepted,true);
  const summary=buildSmokeSummary(raw,gate,retries,{environment:'github-hosted'});
  assert.equal(summary.full_catalog_attempted,true);
  assert.equal(summary.complete_source_verification,false);
  assert.equal(summary.release_verification.status,'not_evaluated');
});
test('missing evidence and clean cloud run cannot imply Mac release verification', () => {
  const missing=buildSmokeSummary(null,null);
  assert.equal(missing.full_catalog_attempted,false);
  assert.equal(missing.complete_source_verification,false);
  assert.equal(missing.gate_accepted,null);
  const raw=report([]),gate=evaluateSourceSmoke(raw,[]);
  const clean=buildSmokeSummary(raw,gate,[],{environment:'github-hosted'});
  assert.equal(clean.complete_source_verification,true);
  assert.equal(clean.release_verification.status,'not_evaluated');
});

test('environment evidence creates its directory and never dumps credentials', async () => {
  const fs=await import('node:fs'),os=await import('node:os'),path=await import('node:path');
  const {spawnSync}=await import('node:child_process');
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'smoke-environment-'));
  try {
    const output=path.join(root,'new-directory','environment-versions.json');
    const sentinel='private-value-must-never-appear';
    const run=spawnSync(process.execPath,['scripts/source-smoke-environment.mjs',output],
      {encoding:'utf8',env:{...process.env,GH_TOKEN:sentinel}});
    assert.equal(run.status,0,run.stderr);
    const raw=fs.readFileSync(output,'utf8'), evidence=JSON.parse(raw);
    assert.equal(evidence.platform,process.platform);
    for (const name of ['chrome','openssl','curl']) assert.equal(typeof evidence[name].available,'boolean');
    assert.equal(raw.includes(sentinel),false);
    assert.equal(run.stdout.includes(sentinel),false);
    assert.equal(evidence.node_version,process.version);
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});
