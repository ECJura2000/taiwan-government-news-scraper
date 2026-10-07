import {test} from 'node:test';
import assert from 'node:assert/strict';
import {evaluateGate} from './release-source-gate.mjs';
const names = Array.from({length:88}, (_,i)=>`source-${i}`);
const report = failed => ({status:failed.length?'partial_failure':'success',report_schema_version:4,selected_source_count:88,selected_sources:names,news_count:10,failed_sources:failed,quality:{alert_reasons:[]},relevance_policy:{ruleset_hash:'a'.repeat(64)}});
const options = {criticalSources:[]};
const retry = (source, recovered, failure='runner_network') => ({source,exit_code:recovered?0:1,retry_status:recovered?'success':'partial_failure',failed_sources:recovered?[]:[source],failure_class_counts:recovered?{}:{[failure]:1}});
test('initial parser failure may recover on source-only retry',()=>assert.equal(evaluateGate(report([names[0]]),[retry(names[0],true)],options).accepted,true));
test('missing evidence and persistent parser failure are rejected',()=>{
 assert.equal(evaluateGate(report([names[0]]),[],options).accepted,false);
 assert.equal(evaluateGate(report([names[0]]),[retry(names[0],false,'parser_regression')],options).accepted,false);
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
