<script lang="ts">
  import { invoke, isPreview } from "./lib/desktop";
  import { previousCompleteWeek, rangeError } from "./lib/date-range";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onDestroy, onMount } from "svelte";
  import type { ProgressEvent, RunOptions, RunSummary } from "./lib/contracts";
  import { advanceCompleted, advanceRunPercent, calculateRunPercent } from "./lib/progress";
  import {
    buildSourceHealthDetails,
    healthTooltip,
    toggleHealthPanel,
    type HealthPanel,
    type SourceHealthDetail,
  } from "./lib/source-health";

  import TopicManager from "./TopicManager.svelte";
  import ThemeToggle from "./ThemeToggle.svelte";
  import { enabledCount, type TopicPolicy } from "./lib/topic-policy";
  let topicsPolicy: TopicPolicy | null = null;
  let topicsPending = true;
  let sources: string[] = [];
  let selectedSources: string[] = [];
  let maxWorkers = 8;
  let outputDir = "";
  let reportDir = "";
  let date = "";
  let periodMode = "previous";
  let topicsOpen = false;
  let sourcesOpen = false;
  let sourceSearch = "";
  let startDate = "";
  let endDate = "";
  let dedupeAffiliated = false;
  let failOnSourceError = false;
  let running = false;
  let loadingSources = true;
  let progress: ProgressEvent | null = null;
  let summary: RunSummary | null = null;
  let error = "";
  let unlisten: UnlistenFn | undefined;
  let startedAt = 0;
  let elapsedSeconds = 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  let lastCompleted = 0;
  let lastPercent = 0;
  let startedSources = new Set<string>();
  let jsonFollowsExcel = true;
  let defaultOutputDir = "";
  let activeHealthPanel: HealthPanel | null = null;
  let unstableDetails: SourceHealthDetail[] = [];
  let failedDetails: SourceHealthDetail[] = [];
  let activeHealthDetails: SourceHealthDetail[] = [];

  async function loadSources() {
    loadingSources = true;
    try {
      defaultOutputDir = await invoke<string>("default_output_dir");
      sources = await invoke<string[]>("list_sources");
      selectedSources = [...sources];
    } catch (cause) {
      error = String(cause);
    } finally {
      loadingSources = false;
    }
  }

  async function runScraper() {
    if (isPreview || running || loadingSources || !selectedSources.length || !topicsPolicy || topicsPending || !enabledCount(topicsPolicy)) return;
    if (periodMode === "custom" && rangeError(startDate, endDate)) {
      error = rangeError(startDate, endDate); return;
    }
    if (periodMode === "week" && !date) { error = "請選擇指定週次的日期。"; return; }
    const previous = previousCompleteWeek();
    running = true;
    summary = null;
    activeHealthPanel = null;
    error = "";
    lastCompleted = 0;
    lastPercent = 0;
    startedSources = new Set();
    elapsedSeconds = 0;
    startedAt = Date.now();
    progress = { kind: "started", completed: 0, total: selectedSources.length, message: "正在準備執行環境" };
    timer = setInterval(() => {
      elapsedSeconds = Math.floor((Date.now() - startedAt) / 1000);
    }, 1000);
    try {
      const options: RunOptions = {
        topics_policy: topicsPolicy,
        sources: selectedSources.length === sources.length ? [] : selectedSources,
        output_dir: outputDir || undefined,
        report_dir: jsonFollowsExcel ? undefined : reportDir || undefined,
        date: periodMode === "week" ? date : undefined,
        start_date: periodMode === "previous" ? previous.start : periodMode === "custom" ? startDate : undefined,
        end_date: periodMode === "previous" ? previous.end : periodMode === "custom" ? endDate : undefined,
        max_workers: maxWorkers,
        dedupe_affiliated: dedupeAffiliated,
        fail_on_source_error: failOnSourceError,
      };
      summary = await invoke<RunSummary>("run_scrape", { options });
    } catch (cause) {
      error = String(cause);
    } finally {
      running = false;
      if (timer) {
        clearInterval(timer);
        timer = undefined;
      }
    }
  }

  async function cancelScraper() {
    try {
      applyProgress({ kind: "cancelling", message: "正在安全停止；不會寫出未完成的報告" });
      await invoke("cancel_run");
    } catch (cause) {
      error = String(cause);
    }
  }

  function toggleSource(source: string) {
    selectedSources = selectedSources.includes(source)
      ? selectedSources.filter((item) => item !== source)
      : [...selectedSources, source];
  }

  function normalizeDialogPath(path: string | string[] | null): string {
    if (Array.isArray(path)) return path[0] ?? "";
    return path ?? "";
  }

  async function chooseOutputDir() {
    const path = normalizeDialogPath(await openDialog({ directory: true, multiple: false }));
    if (path) outputDir = path;
  }

  async function chooseReportDir() {
    const path = normalizeDialogPath(await openDialog({ directory: true, multiple: false }));
    if (path) {
      reportDir = path;
      jsonFollowsExcel = false;
    }
  }

  function useDefaultOutputDir() {
    outputDir = "";
  }

  function followExcelDir() {
    reportDir = "";
    jsonFollowsExcel = true;
  }

  async function revealPath(path: string) {
    try {
      await revealItemInDir(path);
    } catch (cause) {
      error = String(cause);
    }
  }

  async function openWebsite(url: string) {
    try {
      await openUrl(url);
    } catch (cause) {
      error = String(cause);
    }
  }

  function selectHealthPanel(panel: HealthPanel, detailCount: number) {
    activeHealthPanel = toggleHealthPanel(activeHealthPanel, panel, detailCount);
  }

  function formatSeconds(value: number): string {
    return `${value.toFixed(value >= 10 ? 1 : 2)} 秒`;
  }

  function applyProgress(event: ProgressEvent) {
    lastCompleted = advanceCompleted(lastCompleted, event);
    lastPercent = advanceRunPercent(
      lastPercent,
      event,
      lastCompleted,
      event.total ?? selectedSources.length,
    );
    if (event.kind === "source_started" && event.source) {
      startedSources = new Set(startedSources).add(event.source);
    }
    progress = event;
  }

  $: previousWeek = previousCompleteWeek();
  $: dateError = periodMode === "custom" ? rangeError(startDate, endDate) : periodMode === "week" && !date ? "請選擇指定週次的日期。" : "";
  $: filteredSources = sources.filter(source => source.includes(sourceSearch.trim()));
  $: currentStep = summary ? 3 : running ? 2 : 1;
  $: statusLabel = (summary ? ({success:"執行完成",attention:"需注意",partial_failure:"部分來源失敗",failure:"執行失敗"}[summary.status]) : null) ?? (progress?.kind === "cancelled" ? "已取消" : running ? "執行中" : loadingSources ? "載入中" : "尚未執行");
  $: progressTotal = progress?.total ?? selectedSources.length;
  $: progressCompleted = Math.max(lastCompleted, progress?.completed ?? 0);
  $: runPercent = Math.max(lastPercent, calculateRunPercent(progress, progressCompleted, progressTotal));
  $: processingSourceCount = Math.max(0, startedSources.size - progressCompleted);
  $: activeSource = progress?.source ?? "";
  $: progressMessage = progress?.message ?? progress?.kind ?? "尚未開始";
  $: effectiveOutputDir = outputDir || defaultOutputDir;
  $: reportPlaceholder =
    jsonFollowsExcel && effectiveOutputDir
      ? `預設：${effectiveOutputDir}/執行紀錄`
      : jsonFollowsExcel
        ? "跟隨 Excel 資料夾下的執行紀錄"
        : "使用指定 JSON 資料夾";
  $: unstableDetails = summary ? buildSourceHealthDetails(summary, "unstable") : [];
  $: failedDetails = summary ? buildSourceHealthDetails(summary, "failed") : [];
  $: activeHealthDetails = activeHealthPanel === "unstable" ? unstableDetails : activeHealthPanel === "failed" ? failedDetails : [];
  $: activeHealthTitle = activeHealthPanel === "unstable" ? "不穩定來源明細" : "失敗來源明細";

  onMount(() => {
    loadSources();
    if (isPreview) return;
    listen<ProgressEvent>("scraper-progress", (event) => {
      applyProgress(event.payload);
    }).then((cleanup) => (unlisten = cleanup)).catch((cause) => { error = `無法接收搜集進度：${String(cause)}`; });
  });

  onDestroy(() => {
    unlisten?.();
    if (timer) clearInterval(timer);
  });
</script>

<main class="shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">中華民國・公開資訊</p>
      <h1>各機關新聞整理</h1>
      <p class="subtitle">中華民國各機關公開新聞彙整</p>
    </div>
    <div class="header-actions"><ThemeToggle /><div class="status-pill" data-status={summary?.status ?? "idle"}>{statusLabel}</div></div>
  </header>

  {#if isPreview}<p class="preview-notice" role="status">介面預覽：顯示公開來源及預設主題；搜集與檔案操作請在桌面程式執行。</p>{/if}
  <nav class="workflow" aria-label="搜集流程">
    {#each ["範圍確認", "搜集新聞", "查看報告"] as step, index}
      <div class:active={currentStep === index + 1} class:done={currentStep > index + 1} aria-current={currentStep === index + 1 ? "step" : undefined}>
        <span class="step-number">{index + 1}</span><span>{step}</span>
      </div>
    {/each}
  </nav>
  <section class="collection-workspace" aria-labelledby="collection-title">
    <div class="workspace-heading">
      <h2 id="collection-title">{running ? "正在搜集各機關新聞" : summary ? "本次搜集已結束" : "確認本次搜集範圍"}</h2>
      <p>請設定新聞期間、搜尋主題、新聞來源與儲存位置，確認後即可開始搜集。</p>
    </div>
    <div class="setup-row period-row">
      <label for="period-mode" class="row-title">新聞期間</label>
      <div class="row-content">
        <div class="period-controls">
          <select id="period-mode" bind:value={periodMode} disabled={running}>
            <option value="previous">上一完整週　{previousWeek.start} — {previousWeek.end}</option>
            <option value="week">指定週次</option>
            <option value="custom">自訂期間</option>
          </select>
          <button class="text-button" disabled={running} onclick={() => periodMode = periodMode === "custom" ? "previous" : "custom"}>{periodMode === "custom" ? "使用上一完整週" : "自訂期間"}</button>
        </div>
        {#if periodMode === "week"}<label class="field range-field">指定日期<input type="date" bind:value={date} disabled={running} /><small>搜集該日期所在的星期一至星期日。</small></label>{/if}
        {#if periodMode === "custom"}<div class="date-range range-field"><label class="field">起始日期<input type="date" bind:value={startDate} disabled={running} /></label><label class="field">結束日期<input type="date" bind:value={endDate} disabled={running} /></label></div>{/if}
        <p class="row-hint">{dateError || (periodMode === "previous" ? `${previousWeek.start} 至 ${previousWeek.end}，星期一至星期日。` : "依選定期間搜集各機關發布之公開新聞。")}</p>
      </div>
    </div>
    <div class="setup-row">
      <h3 class="row-title">搜尋主題</h3>
      <div class="row-content"><strong class="row-value">{topicsPending ? "載入主題中" : enabledCount(topicsPolicy) + " 個已啟用"}</strong><p class="row-hint">依設定的主題關鍵詞，搜尋各機關新聞內容。</p></div>
      <button class="text-button" disabled={running} aria-expanded={topicsOpen} aria-controls="topic-settings" onclick={() => topicsOpen = !topicsOpen}>{topicsOpen ? "收合設定" : "前往設定"}</button>
    </div>
    <div id="topic-settings" class="expanded-settings" hidden={!topicsOpen}><TopicManager bind:profile={topicsPolicy} busy={running} bind:pending={topicsPending} /></div>
    <div class="setup-row">
      <h3 class="row-title">新聞來源</h3>
      <div class="row-content"><strong class="row-value">{loadingSources ? "載入來源中" : selectedSources.length + " 個已選取"}</strong><p class="row-hint">{selectedSources.length === 0 && !loadingSources ? "請至少選取一個新聞來源。" : "依所選取的機關網站取得公開新聞。"}</p></div>
      <button class="text-button" disabled={running} aria-expanded={sourcesOpen} aria-controls="source-settings" onclick={() => sourcesOpen = !sourcesOpen}>{sourcesOpen ? "收合設定" : "前往設定"}</button>
    </div>
    <section id="source-settings" class="expanded-settings" hidden={!sourcesOpen} aria-label="新聞來源設定">
      <div class="card-heading"><div><h3>選擇新聞來源</h3><p>{selectedSources.length} / {sources.length} 個來源已選取</p></div><div class="button-row"><button disabled={running} onclick={() => selectedSources = [...sources]}>全選</button><button disabled={running} onclick={() => selectedSources = []}>清除</button></div></div>
      <label class="field">搜尋機關<input type="search" bind:value={sourceSearch} placeholder="輸入機關名稱" /></label>
      <div class="source-list">{#each filteredSources as source}<label class:selected={selectedSources.includes(source)}><input type="checkbox" disabled={running} checked={selectedSources.includes(source)} onchange={() => toggleSource(source)} /><span>{source}</span></label>{/each}</div>
      {#if filteredSources.length === 0}<p class="inline-hint">找不到符合的機關，請更換搜尋詞。</p>{/if}
    </section>
    <div class="setup-row storage-row">
      <label class="row-title" for="output-dir">儲存位置</label>
      <div class="row-content"><div class="storage-controls"><input id="output-dir" value={outputDir || "預設新聞搜集區"} title={effectiveOutputDir} readonly /><button disabled={running || isPreview} onclick={chooseOutputDir}>選擇資料夾</button></div><p class="row-hint">搜集完成後，Excel 報告將儲存至此資料夾。{#if outputDir}<button class="text-button inline-reset" disabled={running} onclick={useDefaultOutputDir}>使用預設</button>{/if}</p></div>
    </div>
    <details class="advanced-settings"><summary>進階設定</summary><fieldset disabled={running} class="settings-fields advanced-fields">
      <label class="field">並行來源數<input type="number" min="1" max="32" bind:value={maxWorkers} /></label>
      <label class="field">執行紀錄儲存位置<div class="input-row"><input bind:value={reportDir} placeholder={reportPlaceholder} readonly /><button type="button" disabled={isPreview} onclick={chooseReportDir}>選擇資料夾</button><button type="button" onclick={followExcelDir}>跟隨 Excel</button></div></label>
      <label class="check-row"><input type="checkbox" bind:checked={dedupeAffiliated} /> 合併部會與所屬機關重複新聞</label>
      <label class="check-row"><input type="checkbox" bind:checked={failOnSourceError} /> 任一來源失敗時標示執行失敗</label>
    </fieldset></details>
    <footer class="collection-actions"><p>{isPreview ? "此為介面預覽，請在桌面程式開始搜集。" : !enabledCount(topicsPolicy) && !topicsPending ? "請先啟用至少一個搜尋主題。" : "開始搜集後，系統會取得新聞並整理報告；完成後可查看來源狀態及報告檔案。"}</p>
      {#if running}<button class="danger" onclick={cancelScraper} disabled={progress?.kind === "cancelling"}>{progress?.kind === "cancelling" ? "正在安全停止" : "停止搜集"}</button>{:else}<button class="primary" onclick={runScraper} disabled={isPreview || selectedSources.length === 0 || !enabledCount(topicsPolicy) || topicsPending || loadingSources || Boolean(dateError)}>開始搜集</button>{/if}
    </footer>
    {#if progress}<div class="progress-box" role="status"><div class="progress-heading"><strong>{progressMessage}</strong><span class="progress-percent">{runPercent}%</span></div><div class="progress-track" class:indeterminate={running && progressCompleted === 0 && runPercent === 0}><div style={`width: ${runPercent}%`}></div></div><div class="progress-meta"><span>已完成：{progressCompleted} / {progressTotal} 個來源</span>{#if activeSource}<span>目前：{activeSource}</span>{/if}{#if running}<span>處理中：{processingSourceCount} 個來源・耗時：{elapsedSeconds} 秒</span>{/if}</div></div>{/if}
  </section>

  {#if error}
    <section class="notice error"><strong>執行錯誤</strong><span>{error}</span></section>
  {/if}

  {#if summary}
    <section class="card results-card">
      <div class="card-heading">
        <div>
          <h2>執行結果</h2>
          <p>請確認來源狀態及內容完整度，再使用本次報告。</p>
        </div>
        <strong class="news-count">{summary.news_count} 筆新聞</strong>
      </div>
      <div class="metrics">
        <div class="metric-card"><span>健康來源</span><strong>{summary.source_health.healthy_count}</strong></div>
        {#if unstableDetails.length > 0}
          <button
            type="button"
            class="metric-card metric-button unstable"
            data-active={activeHealthPanel === "unstable"}
            aria-expanded={activeHealthPanel === "unstable"}
            aria-controls="source-health-details"
            aria-label={`查看 ${unstableDetails.length} 個不穩定來源`}
            onclick={() => selectHealthPanel("unstable", unstableDetails.length)}
          >
            <span>不穩定</span><strong>{summary.source_health.unstable_count}</strong>
            <small>移入查看，按下展開</small>
            <span class="metric-tooltip" role="tooltip">
              <b>不穩定網站</b>
              {#each healthTooltip(unstableDetails) as item}<span>{item}</span>{/each}
            </span>
          </button>
        {:else}
          <div class="metric-card"><span>不穩定</span><strong>{summary.source_health.unstable_count}</strong></div>
        {/if}
        {#if failedDetails.length > 0}
          <button
            type="button"
            class="metric-card metric-button failed"
            data-active={activeHealthPanel === "failed"}
            aria-expanded={activeHealthPanel === "failed"}
            aria-controls="source-health-details"
            aria-label={`查看 ${failedDetails.length} 個失敗來源`}
            onclick={() => selectHealthPanel("failed", failedDetails.length)}
          >
            <span>失敗來源</span><strong>{summary.source_health.failed_count}</strong>
            <small>移入查看，按下展開</small>
            <span class="metric-tooltip" role="tooltip">
              <b>失敗網站</b>
              {#each healthTooltip(failedDetails) as item}<span>{item}</span>{/each}
            </span>
          </button>
        {:else}
          <div class="metric-card"><span>失敗來源</span><strong>{summary.source_health.failed_count}</strong></div>
        {/if}
        <div class="metric-card"><span>品質告警</span><strong>{summary.quality.alert_reasons?.length ?? 0}</strong></div>
        {#if summary.content_mode !== "summary" && summary.quality.full_text_count !== undefined}
          <div class="metric-card"><span>取得全文</span><strong>{summary.quality.full_text_count} / {summary.news_count}</strong><small>覆蓋率 {((summary.quality.full_text_coverage_rate ?? 0) * 100).toFixed(1)}%</small></div>
          <div class="metric-card"><span>以摘要補位</span><strong>{summary.quality.description_fallback_count ?? 0}</strong></div>
          <div class="metric-card"><span>全文補取失敗或空白</span><strong>{summary.quality.detail_fetch_failed_or_empty_count ?? 0} / {summary.quality.detail_fetch_attempted_count ?? 0}</strong></div>
        {/if}
      </div>
      {#if summary.content_mode === "summary"}
        <p>本次使用摘要模式，未補取全文。</p>
      {:else if summary.quality.content_warnings?.includes("low_full_text_coverage")}
        <p role="status">全文覆蓋率低於 50%；Excel 已標示以列表摘要補位及未取得內容的新聞。來源列表成功不代表全文取得成功。</p>
      {/if}
      {#if activeHealthPanel && activeHealthDetails.length > 0}
        <section class="health-details" id="source-health-details" aria-live="polite">
          <div class="health-details-heading">
            <div>
              <p class="eyebrow">SOURCE HEALTH</p>
              <h3>{activeHealthTitle}</h3>
            </div>
            <button class="quiet compact" type="button" onclick={() => (activeHealthPanel = null)}>收合</button>
          </div>
          <div class="health-detail-list">
            {#each activeHealthDetails as detail}
              <article class="health-detail">
                <div class="health-detail-title">
                  <div>
                    <strong>{detail.source}</strong>
                    <span>{detail.host || "網站未記錄"}</span>
                  </div>
                  <span class:failed-badge={detail.status === "failed"} class:unstable-badge={detail.status === "unstable"}>
                    {detail.status === "failed" ? "最終失敗" : "已恢復但不穩定"}
                  </span>
                </div>
                <dl class="health-facts">
                  <div><dt>原因</dt><dd>{detail.failureLabel}</dd></div>
                  <div><dt>重試</dt><dd>{detail.retryCount} 次</dd></div>
                  <div><dt>取得筆數</dt><dd>{detail.itemCount}</dd></div>
                  <div><dt>耗時</dt><dd>{formatSeconds(detail.elapsedSeconds)}</dd></div>
                </dl>
                <p class="health-message">{detail.message}</p>
                <div class="route-summary">
                  <span>問題路由：{detail.url || "未記錄"}</span>
                  {#if detail.usedFallback || detail.finalRouteId}
                    <span>
                      最終路由：{detail.finalRouteId || "未記錄"}{detail.finalHost ? ` · ${detail.finalHost}` : ""}{detail.usedFallback ? "（備援）" : ""}
                    </span>
                  {/if}
                </div>
                <div class="button-row health-actions">
                  {#if detail.url}
                    <button class="quiet compact" type="button" onclick={() => openWebsite(detail.url)}>開啟問題網站</button>
                  {/if}
                  {#if detail.finalUrl && detail.finalUrl !== detail.url}
                    <button class="quiet compact" type="button" onclick={() => openWebsite(detail.finalUrl)}>開啟最終網站</button>
                  {/if}
                </div>
                {#if detail.attempts.length > 0}
                  <details class="attempt-list">
                    <summary>查看 {detail.attempts.length} 次路由紀錄</summary>
                    {#each detail.attempts as attempt}
                      <div class="attempt-row">
                        <span class:attempt-failed={attempt.status === "failed"}>{attempt.status === "failed" ? "失敗" : "成功"}</span>
                        <code>{attempt.route_id || "未命名路由"} · 第 {attempt.attempt_number ?? 1} 次</code>
                        <span>{formatSeconds(attempt.elapsed_seconds ?? 0)}</span>
                      </div>
                    {/each}
                  </details>
                {/if}
              </article>
            {/each}
          </div>
        </section>
      {/if}
      <div class="paths">
        <div><span>Excel</span><code>{summary.output_file || "未產生"}</code></div>
        <div><span>報告</span><code>{summary.report_file || "未產生"}</code></div>
      </div>
      <div class="button-row result-actions">
        {#if summary.output_file}
          <button class="quiet" onclick={() => revealPath(summary?.output_file ?? "")}>開啟 Excel 所在資料夾</button>
        {/if}
        {#if summary.report_file}
          <button class="quiet" onclick={() => revealPath(summary?.report_file ?? "")}>開啟 JSON 所在資料夾</button>
        {/if}
      </div>
    </section>
  {/if}
</main>
