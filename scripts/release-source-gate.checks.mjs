import {test} from 'node:test';
import assert from 'node:assert/strict';
import {evaluateGate} from './release-source-gate.mjs';
import {loadApprovedSources, matchesApprovedSources, validateSourceNames, checkSourceCatalog} from './check-source-catalog.mjs';
const names = loadApprovedSources();
const report = failed => ({status:failed.length?'partial_failure':'success',report_schema_version:4,selected_source_count:names.length,selected_sources:names,news_count:10,failed_sources:failed,quality:{alert_reasons:[]},relevance_policy:{ruleset_hash:'a'.repeat(64)}});
const options = {criticalSources:[]};
const retry = (source, recovered, failure='runner_network') => ({source,exit_code:recovered?0:1,retry_status:recovered?'success':'partial_failure',failed_sources:recovered?[]:[source],failure_class_counts:recovered?{}:{[failure]:1}});
test('initial parser failure may recover on source-only retry',()=>assert.equal(evaluateGate(report([names[0]]),[retry(names[0],true)],options).accepted,true));
test('missing evidence and persistent parser failure are rejected',()=>{
 assert.equal(evaluateGate(report([names[0]]),[],options).accepted,false);
 assert.equal(evaluateGate(report([names[0]]),[retry(names[0],false,'parser_regression')],options).accepted,false);
});
test('full report must contain the exact approved names, not just the count',()=>{
 const replaced=[...names];replaced[0]='unapproved-source';
 assert.equal(evaluateGate({...report([]),selected_sources:replaced},[],options).accepted,false);
 assert.equal(evaluateGate({...report([]),selected_sources:names.slice(1),selected_source_count:names.length-1},[],options).accepted,false);
 assert.equal(evaluateGate({...report([]),selected_sources:[...names,names[0]],selected_source_count:names.length+1},[],options).accepted,false);
 assert.equal(evaluateGate({...report([]),selected_sources:[...names].reverse()},[],options).accepted,true);
});
test('explicit approved fixtures use their own denominator and validate names',()=>{
 const expectedSources=Array.from({length:10},(_,i)=>`fixture-${i}`);
 const small=failed=>({...report(failed),selected_source_count:expectedSources.length,selected_sources:expectedSources});
 const config={criticalSources:[],expectedSources};
 assert.equal(evaluateGate(small([expectedSources[0]]),[retry(expectedSources[0],false)],config).accepted,true);
 const failed=expectedSources.slice(0,2);
 assert.equal(evaluateGate(small(failed),failed.map(n=>retry(n,false)),config).accepted,false);
 assert.throws(()=>validateSourceNames([]),/Invalid/);
 assert.throws(()=>validateSourceNames(['duplicate','duplicate']),/Invalid/);
 assert.throws(()=>validateSourceNames([' padded ']),/Invalid/);
 assert.equal(matchesApprovedSources(['wrong'],['approved']),false);
});
test('catalog/CLI checks reject same-count replacement and duplicate output',()=>{
 assert.equal(checkSourceCatalog([...names].reverse().join('\r\n')+'\r\n'),names.length);
 const replaced=[...names];replaced[0]='unapproved-source';
 assert.throws(()=>checkSourceCatalog(replaced.join('\n')),/CLI list-sources/);
 const duplicate=[...names];duplicate[0]=duplicate[1];
 assert.throws(()=>checkSourceCatalog(duplicate.join('\n')),/CLI list-sources/);
});
test('count/rate limit uses final failures, with exact boundary',()=>{
 const failed=names.slice(0,9);
 assert.equal(evaluateGate(report(failed),failed.map(n=>retry(n,false)),options).accepted,false);
 assert.equal(evaluateGate(report(failed),failed.map((n,i)=>retry(n,i===0)),options).accepted,true);
});
test('critical source failure blocks release even within numeric budget',()=>assert.equal(evaluateGate(report([names[0]]),[retry(names[0],false)],{criticalSources:[names[0]]}).accepted,false));
test('malformed quality/selected sources and invalid limits fail closed',()=>{
 const r=report([]);r.quality={};assert.equal(evaluateGate(r,[],options).accepted,false);
 assert.throws(()=>evaluateGate(report([]),[],{maxFailed:NaN}),/Invalid/);
});
test('malformed or duplicate failure evidence cannot bypass the gate',()=>{
 for (const failed_sources of [undefined, {}, [names[0], names[0]], ['unselected']]) {
  assert.equal(evaluateGate({...report([]),failed_sources},[],options).accepted,false);
 }
 const evidence=retry(names[0],false);
 for (const change of [{failure_class_counts:{runner_network:0}}, {exit_code:2}, {failed_sources:[]}, {retry_status:'success'}]) {
  assert.equal(evaluateGate(report([names[0]]),[{...evidence,...change}],options).accepted,false);
 }
 assert.equal(evaluateGate(report([names[0]]),[evidence,evidence],options).accepted,false);
});
