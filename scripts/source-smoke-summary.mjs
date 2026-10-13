import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {loadApprovedSources, matchesApprovedSources} from './check-source-catalog.mjs';

// Presentation only: this never recomputes acceptance, removes failures, or
// writes source-health history. The adjacent raw gate remains authoritative.
export function buildSmokeSummary(report, gate, retries = [], {
  environment = 'unknown', expectedSources = loadApprovedSources(), environmentVersions = null,
} = {}) {
  const fullSet = !!report && report.selected_source_count === expectedSources.length &&
    matchesApprovedSources(report.selected_sources ?? [], expectedSources);
  const initial = Array.isArray(report?.failed_sources) ? report.failed_sources : [];
  const unresolved = Array.isArray(gate?.final_failed_sources) ? gate.final_failed_sources : initial;
  const failures = unresolved.map(source => {
    const diagnostic = report?.source_diagnostics?.find(item => item.source === source);
    const retry = retries.find(item => item.source === source);
    const finalClasses = Object.keys(retry?.failure_class_counts ?? {}).sort();
    const cultureBlocked = environment === 'github-hosted' && source === '文化部' && finalClasses.includes('access_blocked');
    return {source, initial_failure_class: diagnostic?.failure_class ?? 'unknown',
      retry_failure_classes: finalClasses, retry_status: retry?.retry_status ?? 'unverified',
      known_environment_limitation: cultureBlocked ? 'official_verification_page_blocks_hosted_runner' : null};
  });
  return {
    version: 1, execution_environment: environment, run_id: report?.run_id ?? null,
    environment_versions: environmentVersions,
    approved_source_count: expectedSources.length,
    selected_source_count: report?.selected_source_count ?? null,
    full_catalog_attempted: fullSet,
    complete_source_verification: fullSet && gate?.accepted === true && unresolved.length === 0,
    gate_accepted: gate?.accepted ?? null,
    gate_errors: gate?.errors ?? ['gate evidence unavailable'],
    gate_evidence: 'gate-result.json',
    verification_scope: gate?.verification_scope ?? null,
    not_collected_in_cloud: gate?.verification_scope?.unverified_sources ?? [],
    initial_failed_sources: initial,
    retry_recovered_sources: initial.filter(source => !unresolved.includes(source)),
    unresolved_failures: failures,
    release_verification: {status: 'not_evaluated', paired_evidence: null,
      required_environment: 'manual-local-mac', required_scope: 'all_approved_sources',
      note: 'A separate full release check on the Mac started at publication is required; this cloud run does not verify release readiness.'},
  };
}

const cell = value => String(value).replaceAll('|', '\\|').replace(/[\r\n]/g, ' ');
export function renderSmokeSummary(summary) {
  const verified = summary.complete_source_verification ? '完整來源已驗證' : '完整來源仍未驗證';
  const lines = ['## 雲端來源檢查範圍', '',
    `執行環境：${cell(summary.execution_environment)}。核准來源：${summary.approved_source_count}；本次選擇：${summary.selected_source_count ?? '未確認'}。`, '',
    `${verified}。原始門檻：${summary.gate_accepted === true ? '通過' : summary.gate_accepted === false ? '未通過' : '缺少證據'}；未恢復來源：${summary.unresolved_failures.length}。`, '',
    '此摘要不改變原始 gate-result.json 的結果。發布另需發布時才啟動的本機 Mac 完整來源檢查；此產物沒有配對的發布驗證證據。', '',
    '文化部的官方驗證頁阻擋會如實列為失敗。其他網路、解析或 Chrome 故障保留當次分類，未視為永久雲端限制，也未排除任何來源。', '',
    'OS、Chrome／Chromium、OpenSSL 與 curl 版本另存 environment-versions.json，供比對實際環境，不含憑證或環境變數內容。'];
  if (summary.not_collected_in_cloud?.length) {
    lines.push('', `本次雲端範圍未收集 ${summary.not_collected_in_cloud.length} 個來源，全部仍保留於核准目錄；須由 Mac 完整驗證，不列為本次成功：`, '',
      '| 須由 Mac 驗證 | 實際原因 | 證據 |', '| --- | --- | --- |');
    for (const row of summary.not_collected_in_cloud) lines.push(`| ${cell(row.source)} | ${row.reason === 'official_verification_page' ? '官方驗證頁阻擋' : 'GitHub TCP／DNS 連線逾時，Mac 成功'} | [雲端證據](${row.network_probe_run}) |`);
  }
  if (summary.unresolved_failures.length) {
    lines.push('', '| 未恢復來源 | 初次分類 | 重測分類 | 環境限制證據 |', '| --- | --- | --- | --- |');
    for (const row of summary.unresolved_failures) lines.push(`| ${cell(row.source)} | ${cell(row.initial_failure_class)} | ${cell(row.retry_failure_classes.join(', ') || row.retry_status)} | ${row.known_environment_limitation ? '官方驗證頁阻擋 GitHub 執行器' : '未確認永久限制'} |`);
  }
  if (summary.gate_errors.length) lines.push('', '原始門檻錯誤：', ...summary.gate_errors.map(error => `- ${cell(error)}`));
  return lines.join('\n') + '\n';
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = process.argv[2];
  if (!root) throw Error('Usage: source-smoke-summary.mjs ARTIFACT_ROOT');
  const read = file => fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : null;
  const reports = path.join(root, 'reports');
  const latest = fs.existsSync(reports) ? fs.readdirSync(reports).filter(name => /^news_scraper_run_.*\.json$/.test(name)).sort().at(-1) : null;
  const retriesRoot = path.join(root, 'retries');
  const retries = fs.existsSync(retriesRoot) ? fs.readdirSync(retriesRoot).map(dir => read(path.join(retriesRoot, dir, 'retry-evidence.json'))).filter(Boolean) : [];
  const summary = buildSmokeSummary(latest ? read(path.join(reports, latest)) : null,
    read(path.join(root, 'gate-result.json')), retries,
    {environment: process.env.SOURCE_SMOKE_EXECUTION_ENVIRONMENT ?? 'unknown',
      environmentVersions:read(path.join(root, 'environment-versions.json'))});
  fs.mkdirSync(root, {recursive: true});
  fs.writeFileSync(path.join(root, 'environment-summary.json'), JSON.stringify(summary, null, 2) + '\n');
  const markdown = renderSmokeSummary(summary);
  fs.writeFileSync(path.join(root, 'environment-summary.md'), markdown);
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown);
  console.log(JSON.stringify(summary));
}
