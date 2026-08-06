<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  let progress: ProgressEvent | null = null;
  let summary: RunSummary | null = null;
  let error = "";
  let unlisten: UnlistenFn | undefined;

  async function loadSources() {
    try {
      sources = await invoke<string[]>("list_sources");
      selectedSources = [...sources];
    } catch (cause) {
      error = String(cause);
    }
  }

  async function runScraper() {
    running = true;
    summary = null;
    error = "";
    progress = { kind: "started", total: selectedSources.length };
    try {
      const options: RunOptions = {
        sources: selectedSources.length === sources.length ? [] : selectedSources,
        output_dir: outputDir || undefined,
        report_dir: reportDir || undefined,
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

  $: statusLabel = summary?.status ?? (progress?.kind === "cancelled" ? "已取消" : running ? "執行中" : "尚未執行");

  loadSources();
  listen<ProgressEvent>("scraper-progress", (event) => {
    progress = event.payload;
  }).then((cleanup) => (unlisten = cleanup));

  // Svelte calls this cleanup when the component is destroyed.
  const cleanup = () => unlisten?.();
</script>

<svelte:window onbeforeunload={cleanup} />

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
          <p>{selectedSources.length} / {sources.length} 個來源已選取</p>
        </div>
        <div class="button-row">
          <button class="quiet" onclick={() => (selectedSources = [...sources])}>全選</button>
          <button class="quiet" onclick={() => (selectedSources = [])}>清除</button>
        </div>
      </div>
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
        <input bind:value={outputDir} placeholder="使用預設資料夾" />
      </label>
      <label class="field">
        <span>JSON 報告資料夾（可選）</span>
        <input bind:value={reportDir} placeholder="使用 Excel 資料夾下的執行紀錄" />
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
          <strong>{progress.message ?? progress.kind}</strong>
          {#if progress.total}
            <span>{progress.completed ?? 0} / {progress.total}</span>
          {/if}
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
    </section>
  {/if}
</main>
