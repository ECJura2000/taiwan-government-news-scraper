<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onDestroy, onMount } from "svelte";
  import type { ProgressEvent, RunOptions, RunSummary } from "./lib/contracts";

  let sources: string[] = [];
  let selectedSources: string[] = [];
  let maxWorkers = 8;
  let outputDir = "";
  let reportDir = "";
  let date = "";
  let startDate = "";
  let endDate = "";
  let dedupeAffiliated = false;
  let failOnSourceError = false;
  let running = false;
  let loadingSources = true;
  let sourceLoadPercent = 0;
  let progress: ProgressEvent | null = null;
  let summary: RunSummary | null = null;
  let error = "";
  let unlisten: UnlistenFn | undefined;
  let startedAt = 0;
  let elapsedSeconds = 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  let lastCompleted = 0;
  let jsonFollowsExcel = true;

  async function loadSources() {
    loadingSources = true;
    sourceLoadPercent = 10;
    try {
      sources = await invoke<string[]>("list_sources");
      sourceLoadPercent = 100;
      selectedSources = [...sources];
    } catch (cause) {
      error = String(cause);
    } finally {
      loadingSources = false;
    }
  }

  async function runScraper() {
    running = true;
    summary = null;
    error = "";
    lastCompleted = 0;
    elapsedSeconds = 0;
    startedAt = Date.now();
    progress = { kind: "started", total: selectedSources.length };
    timer = setInterval(() => {
      elapsedSeconds = Math.floor((Date.now() - startedAt) / 1000);
    }, 1000);
    try {
      const options: RunOptions = {
        sources: selectedSources.length === sources.length ? [] : selectedSources,
        output_dir: outputDir || undefined,
        report_dir: jsonFollowsExcel ? undefined : reportDir || undefined,
        date: date || undefined,
        start_date: startDate || undefined,
        end_date: endDate || undefined,
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
      progress = { kind: "cancelling", message: "正在安全停止；不會寫出未完成的報告" };
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

  $: statusLabel = summary?.status ?? (progress?.kind === "cancelled" ? "已取消" : running ? "執行中" : loadingSources ? "載入中" : "尚未執行");
  $: progressTotal = progress?.total ?? selectedSources.length;
  $: progressCompleted = progress?.completed ?? lastCompleted;
  $: if (progress?.completed !== undefined) lastCompleted = progress.completed;
  $: runPercent = progressTotal ? Math.min(100, Math.round((progressCompleted / progressTotal) * 100)) : 0;
  $: activeSource = progress?.source ?? "";
  $: progressMessage = progress?.message ?? progress?.kind ?? "尚未開始";
  $: reportPlaceholder = jsonFollowsExcel ? "跟隨 Excel 資料夾下的執行紀錄" : "使用指定 JSON 資料夾";

  onMount(() => {
    loadSources();
    listen<ProgressEvent>("scraper-progress", (event) => {
      progress = event.payload;
    }).then((cleanup) => (unlisten = cleanup));
  });

  onDestroy(() => {
    unlisten?.();
    if (timer) clearInterval(timer);
  });
</script>

<main class="shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">TAIWAN GOVERNMENT NEWS</p>
      <h1>各機關新聞整理</h1>
      <p class="subtitle">Rust + Tauri v2 遷移版</p>
    </div>
    <div class="status-pill" data-status={summary?.status ?? "idle"}>{statusLabel}</div>
  </header>

  <section class="grid">
    <article class="card sources-card">
      <div class="card-heading">
        <div>
          <h2>來源</h2>
          {#if loadingSources}
            <p>載入來源 {sourceLoadPercent}%</p>
          {:else}
            <p>{selectedSources.length} / {sources.length} 個來源已選取</p>
          {/if}
        </div>
        <div class="button-row">
          <button class="quiet" onclick={() => (selectedSources = [...sources])}>全選</button>
          <button class="quiet" onclick={() => (selectedSources = [])}>清除</button>
        </div>
      </div>
      {#if loadingSources}
        <div class="progress-track load-track"><div style={`width: ${sourceLoadPercent}%`}></div></div>
      {/if}
      <div class="source-list">
        {#each sources as source}
          <label class:selected={selectedSources.includes(source)}>
            <input
              type="checkbox"
              checked={selectedSources.includes(source)}
              onchange={() => toggleSource(source)}
            />
            <span>{source}</span>
          </label>
        {/each}
      </div>
    </article>

    <article class="card controls-card">
      <div class="card-heading">
        <div>
          <h2>執行設定</h2>
          <p>相容於既有 headless pipeline</p>
        </div>
      </div>
      <label class="field">
        <span>並行來源數</span>
        <input type="number" min="1" max="32" bind:value={maxWorkers} />
      </label>
      <label class="field">
        <span>Excel 輸出資料夾（可選）</span>
        <div class="input-row">
          <input bind:value={outputDir} placeholder="使用預設資料夾" readonly />
          <button type="button" class="quiet" onclick={chooseOutputDir}>選擇資料夾</button>
          <button type="button" class="quiet" onclick={useDefaultOutputDir}>使用預設</button>
        </div>
      </label>
      <label class="field">
        <span>JSON 報告資料夾（可選）</span>
        <div class="input-row">
          <input bind:value={reportDir} placeholder={reportPlaceholder} readonly />
          <button type="button" class="quiet" onclick={chooseReportDir}>選擇資料夾</button>
          <button type="button" class="quiet" onclick={followExcelDir}>跟隨 Excel</button>
        </div>
      </label>
      <label class="field">
        <span>指定日期（可選）</span>
        <input type="date" bind:value={date} disabled={Boolean(startDate || endDate)} />
      </label>
      <div class="date-range">
        <label class="field">
          <span>起始日期</span>
          <input type="date" bind:value={startDate} disabled={Boolean(date)} />
        </label>
        <label class="field">
          <span>結束日期</span>
          <input type="date" bind:value={endDate} disabled={Boolean(date)} />
        </label>
      </div>
      <label class="check-row"><input type="checkbox" bind:checked={dedupeAffiliated} /> 合併部會與所屬機關重複新聞</label>
      <label class="check-row"><input type="checkbox" bind:checked={failOnSourceError} /> 任一來源失敗時回傳失敗碼</label>
      <div class="action-row">
        {#if running}
          <button class="danger" onclick={cancelScraper}>停止執行</button>
        {:else}
          <button class="primary" onclick={runScraper} disabled={selectedSources.length === 0}>開始抓取</button>
        {/if}
      </div>
      {#if progress}
        <div class="progress-box">
          <div class="progress-heading">
            <strong>{progressMessage}</strong>
            <span>{runPercent}%</span>
          </div>
          <div class="progress-track"><div style={`width: ${runPercent}%`}></div></div>
          <div class="progress-meta">
            <span>{progressCompleted} / {progressTotal} 個來源</span>
            {#if activeSource}<span>目前：{activeSource}</span>{/if}
            {#if running}<span>耗時：{elapsedSeconds} 秒</span>{/if}
          </div>
        </div>
      {/if}
    </article>
  </section>

  {#if error}
    <section class="notice error"><strong>執行錯誤</strong><span>{error}</span></section>
  {/if}

  {#if summary}
    <section class="card results-card">
      <div class="card-heading">
        <div>
          <h2>執行結果</h2>
          <p>本次報告保留來源健康與品質警示，不以 exit code 單獨判斷成功。</p>
        </div>
        <strong class="news-count">{summary.news_count} 筆新聞</strong>
      </div>
      <div class="metrics">
        <div><span>健康來源</span><strong>{summary.source_health.healthy_count}</strong></div>
        <div><span>不穩定</span><strong>{summary.source_health.unstable_count}</strong></div>
        <div><span>失敗來源</span><strong>{summary.source_health.failed_count}</strong></div>
        <div><span>品質告警</span><strong>{summary.quality.alert_reasons?.length ?? 0}</strong></div>
      </div>
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
