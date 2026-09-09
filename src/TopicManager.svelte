<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import { clonePolicy, enabledCount, ruleFields, type TopicPolicy, type Topic, type ImportPreview } from "./lib/topic-policy";
  export let profile: TopicPolicy | null = null;
  export let busy = false;
  export let pending = false;
  let error = "";
  let message = "";
  let importText = "";
  let importName = "";
  let replace = false;
  let preview: ImportPreview | null = null;
  let previewDialog: HTMLDialogElement;
  let editorDialog: HTMLDialogElement;
  let editTopic: Topic | null = null;
  let editIndex = -1;
  let returnFocus: HTMLElement | null = null;
  const locked = () => busy || pending;
  const commonLabels: Record<string, string> = {scoring:"評分參數", thresholds:"相關性門檻", general_keywords:"一般關鍵詞", references:"政策來源", name:"設定名稱", version:"文件版本"};

  onMount(async () => {
    pending = true;
    try { profile = await invoke<TopicPolicy>("load_topic_policy"); }
    catch (cause) { error = `無法載入主題設定：${String(cause)}。可匯入設定或恢復預設。`; }
    finally { pending = false; }
  });
  async function persist(next: TopicPolicy) {
    if (locked()) return;
    pending = true; error = "";
    try {
      await invoke("save_topic_policy", { profile: next });
      profile = next; message = "主題設定已儲存，於下次蒐集時適用。";
      return true;
    } catch (cause) { error = `儲存未完成，原設定保留：${String(cause)}`; return false; }
    finally { pending = false; }
  }
  async function toggle(index: number) {
    if (!profile || locked()) return;
    const next = clonePolicy(profile); next.initiatives[index].enabled = !next.initiatives[index].enabled;
    await persist(next);
  }
  async function buildPreview() {
    if (locked()) return;
    pending = true; error = "";
    try {
      const current = profile ?? await invoke<TopicPolicy>("default_topic_policy");
      preview = await invoke<ImportPreview>("preview_topic_import", { current, text: importText, replace });
    } catch (cause) { preview = null; error = String(cause); }
    finally { pending = false; }
  }
  async function showPreview(text: string, name: string, clear: boolean) {
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    importText = text; importName = name; replace = clear;
    await buildPreview(); await tick();
    if (preview) previewDialog.showModal();
  }
  async function importFile(event: Event) {
    const input = event.target as HTMLInputElement; const file = input.files?.[0];
    input.value = "";
    if (!file || locked()) return;
    if (file.size > 2 * 1024 * 1024) { error = "主題 JSON 不得超過 2 MiB。"; return; }
    try { await showPreview(await file.text(), file.name, false); }
    catch (cause) { error = `無法讀取檔案：${String(cause)}`; }
  }
  async function removeTopic(index: number) {
    if (!profile || locked()) return;
    const next = clonePolicy(profile); const removed = next.initiatives.splice(index, 1)[0];
    await showPreview(JSON.stringify(next), `刪除主題：${removed.name}`, true);
  }
  async function defaults() {
    if (locked()) return;
    try { await showPreview(JSON.stringify(await invoke<TopicPolicy>("default_topic_policy")), "恢復預設主題", true); }
    catch (cause) { error = String(cause); }
  }
  async function exportPolicy(example: boolean) {
    if (locked()) return;
    try {
      const outgoing = example ? await invoke<TopicPolicy>("default_topic_policy") : profile;
      if (!outgoing) return;
      const path = await save({ title: example ? "下載主題範例" : "匯出主題設定", defaultPath: example ? "ai-ten-topics.json" : "topics.json", filters: [{ name: "主題 JSON", extensions: ["json"] }] });
      if (path) { await invoke("export_topic_policy", { path, profile: outgoing }); message = "主題 JSON 已匯出。"; }
    } catch (cause) { error = String(cause); }
  }
  function closePreview() { previewDialog.close(); preview = null; returnFocus?.focus(); }
  async function applyPreview() { if (preview && await persist(preview.profile)) closePreview(); }
  async function openEditor(index: number) {
    if (!profile || locked()) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    editIndex = index; editTopic = clonePolicy(profile.initiatives[index]); error = "";
    await tick(); editorDialog.showModal();
  }
  function closeEditor() { editorDialog.close(); editTopic = null; returnFocus?.focus(); }
  async function saveEditor() {
    if (!profile || !editTopic) return;
    if (!editTopic.name.trim()) {error = "請填寫主題名稱。"; return;}
    if (editTopic.penalty_keywords.some(r => !Number.isInteger(r.penalty) || r.penalty < 0 || r.penalty > 100)) {error = "扣分值須為 0 至 100 的整數。"; return;}
    if ([...editTopic.penalty_keywords, ...editTopic.exclude_keywords].some(r => !r.text.trim() || !r.match_fields.length)) {error = "請填寫規則詞句，並至少選取一個比對欄位。"; return;}
    if (editTopic.weighted_keywords.some(k => !k.text.trim() || !Number.isFinite(k.weight) || k.weight <= 0 || k.weight > 100)) {error = "請填寫關鍵詞，權重須大於 0 且不超過 100。"; return;}
    const next = clonePolicy(profile); next.initiatives[editIndex] = editTopic;
    if (await persist(next)) closeEditor();
  }
</script>

<article class="card topics-card" aria-busy={pending}>
  <div class="card-heading"><div><h2>搜尋主題</h2><p>{enabledCount(profile)} / {profile?.initiatives.length ?? 0} 個主題適用・各主題分別產製工作表</p></div></div>
  <div class="topic-toolbar">
    <label class="file-button"><span>匯入 JSON</span><input aria-label="匯入主題 JSON" type="file" accept=".json,application/json" disabled={busy || pending} onchange={importFile} /></label>
    <button class="quiet" disabled={busy || pending || !profile} onclick={() => exportPolicy(false)}>匯出設定</button>
    <button class="quiet" disabled={busy || pending} onclick={() => exportPolicy(true)}>下載範例</button>
    <button class="quiet" disabled={busy || pending} onclick={defaults}>恢復預設</button>
  </div>
  {#if !enabledCount(profile)}<p class="inline-hint">請至少啟用一個搜尋主題，始得開始蒐集。</p>{/if}
  <div class="topic-list">
    {#each profile?.initiatives ?? [] as topic, index}
      <div class="topic-row" class:topic-disabled={!topic.enabled}>
        <label><input type="checkbox" checked={topic.enabled} disabled={busy || pending} onchange={() => toggle(index)} /><span><strong>{topic.name}</strong><small>{topic.weighted_keywords.filter(k => k.enabled).length + topic.strong_keywords.length + topic.context_keywords.length} 個關鍵詞・扣分詞 {topic.penalty_keywords.filter(k => k.enabled).length}・完全排除詞 {topic.exclude_keywords.filter(k => k.enabled).length}</small><small>{[...topic.strong_keywords, ...topic.weighted_keywords.filter(k => k.enabled).slice(0, 4).map(k => k.text)].slice(0,4).join("、")}</small></span></label>
        <div class="button-row"><button class="quiet compact" aria-label={`編輯 ${topic.name} 規則`} disabled={busy || pending} onclick={() => openEditor(index)}>規則</button><button class="delete-button compact" aria-label={`刪除 ${topic.name}`} disabled={busy || pending} onclick={() => removeTopic(index)}>刪除</button></div>
      </div>
    {/each}
  </div>
  {#if message}<p class="inline-hint" role="status">{message}</p>{/if}
  {#if error}<p class="notice" role="alert">{error}</p>{/if}
</article>

<dialog bind:this={previewDialog} class="policy-dialog" oncancel={(e) => {e.preventDefault(); if (!pending) closePreview();}} aria-labelledby="import-title">
  <h2 id="import-title">主題設定變更確認</h2><p class="inline-hint">{importName}</p>
  <label class="field">匯入方式<select bind:value={replace} onchange={buildPreview} disabled={pending}><option value={false}>合併：保留異名主題，同名整筆取代</option><option value={true}>清空後匯入</option></select></label>
  {#if preview}
    <div class="preview-changes"><p><strong>新增：</strong>{preview.added.join("、") || "無"}</p><p><strong>取代：</strong>{preview.updated.join("、") || "無"}</p><p><strong>刪除：</strong>{preview.deleted.join("、") || "無"}</p></div>
    {#each preview.common_changes as change}<details><summary>共用設定變更：{commonLabels[change.field] ?? change.field}</summary><p>原設定</p><pre>{JSON.stringify(change.before, null, 2)}</pre><p>新設定</p><pre>{JSON.stringify(change.after, null, 2)}</pre></details>{/each}
  {/if}
  {#if error}<p class="notice" role="alert">{error}</p>{/if}
  <div class="dialog-actions"><button class="quiet" disabled={pending} onclick={closePreview}>取消</button><button class="primary" disabled={pending || !preview} onclick={applyPreview}>確認套用</button></div>
</dialog>
<dialog bind:this={editorDialog} class="policy-dialog" oncancel={(e) => {e.preventDefault(); if (!pending) closeEditor();}} aria-labelledby="rules-title">
  <h2 id="rules-title">主題規則設定</h2>
  {#if editTopic}
    <fieldset class="settings-fields" disabled={pending}>
    <label class="field">主題名稱<input bind:value={editTopic.name} /></label>
    <label class="field">優先關聯機關<input bind:value={editTopic.lead_source} /></label>
    <p class="inline-hint">規則僅適用於本主題。完全排除優先於扣分；其他主題仍可獨立收錄。</p>
    {#each ["penalty_keywords", "exclude_keywords"] as rawKey}
      {@const key = rawKey as "penalty_keywords" | "exclude_keywords"}
      <section class="rule-section"><h3>{key === "penalty_keywords" ? "扣分詞" : "完全排除詞"}</h3>
        {#each editTopic[key] as rule, index}
          <div class="rule-row">
            <label class="rule-enabled"><input aria-label={`啟用${rule.text || '規則'}`} type="checkbox" bind:checked={rule.enabled} />適用</label>
            <input aria-label="詞句" placeholder="輸入詞句" bind:value={rule.text} />
            {#if key === "penalty_keywords"}<label>扣分<input aria-label="扣分值" type="number" min="0" max="100" bind:value={rule.penalty} /></label>{/if}
            <label><input type="checkbox" checked={rule.match_fields.includes("title")} onchange={(e) => {rule.match_fields = ruleFields(rule, "title", e.currentTarget.checked);}} />標題</label>
            <label><input type="checkbox" checked={rule.match_fields.includes("summary")} onchange={(e) => {rule.match_fields = ruleFields(rule, "summary", e.currentTarget.checked);}} />摘要</label>
            <button class="delete-button compact" onclick={() => {if(editTopic) editTopic[key] = editTopic[key].filter((_, i) => i !== index);}}>移除</button>
          </div>
        {/each}
        <button class="quiet" onclick={() => {if(editTopic) editTopic[key] = [...editTopic[key], {text:"", penalty:50, enabled:true, match_fields:["title","summary"]}];}}>新增{key === "penalty_keywords" ? "扣分詞" : "完全排除詞"}</button>
      </section>
    {/each}
    <details class="rule-section"><summary>加權政策詞（{editTopic.weighted_keywords.length}）</summary>
      {#each editTopic.weighted_keywords as word, index}<div class="rule-row"><input aria-label={`啟用 ${word.text}`} type="checkbox" bind:checked={word.enabled} /><input aria-label="關鍵詞" value={word.text} oninput={(e) => {word.text = e.currentTarget.value; word.origin = "custom"; word.references = [];}} /><label>權重<input aria-label="權重" type="number" min="0.1" max="100" step="0.1" bind:value={word.weight} /></label><button class="delete-button compact" onclick={() => {if(editTopic) editTopic.weighted_keywords = editTopic.weighted_keywords.filter((_, i) => i !== index);}}>移除</button></div>{/each}
      <button class="quiet" onclick={() => {if(editTopic) editTopic.weighted_keywords = [...editTopic.weighted_keywords, {text:"", weight:3, enabled:true, origin:"custom", references:[]}];}}>新增加權詞</button>
    </details>
    {#each ["exact_phrases", "strong_keywords", "context_keywords"] as rawKey}
      {@const key = rawKey as "exact_phrases" | "strong_keywords" | "context_keywords"}
      <label class="field">{key === 'exact_phrases' ? '完整片語' : key === 'strong_keywords' ? '核心詞' : '脈絡詞'}（每行一詞）<textarea value={editTopic[key].join('\n')} oninput={(e) => {if(editTopic) editTopic[key] = e.currentTarget.value.split('\n').map(s => s.trim()).filter(Boolean);}}></textarea></label>
    {/each}
    </fieldset>
  {/if}
  {#if error}<p class="notice" role="alert">{error}</p>{/if}
  <div class="dialog-actions"><button class="quiet" disabled={pending} onclick={closeEditor}>取消</button><button class="primary" disabled={pending} onclick={saveEditor}>儲存規則</button></div>
</dialog>
