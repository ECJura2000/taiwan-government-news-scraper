<script lang="ts">
  import { onMount } from "svelte";
  let preference = "system";
  let systemDark = false;
  let textSize = "100";
  function applySize() {
    document.documentElement.style.fontSize = `${14 * Number(textSize) / 100}px`;
    try {localStorage.setItem("news-text-size", textSize);} catch { /* Use default when unavailable. */ }
  }
  function apply() {
    document.documentElement.dataset.theme = preference === "system" ? (systemDark ? "dark" : "light") : preference;
    try { localStorage.setItem("news-theme", preference); } catch { /* WebView storage may be unavailable. */ }
  }
  onMount(() => {
    try { const stored=localStorage.getItem("news-theme"); if(stored && ["system","light","dark"].includes(stored)) preference=stored; } catch { /* Use system default. */ }
    try {const stored=localStorage.getItem("news-text-size"); if(stored && ["100","125","150","200"].includes(stored)) textSize=stored;} catch { /* Use default. */ }
    applySize();
    const query=matchMedia("(prefers-color-scheme: dark)"); systemDark=query.matches; apply();
    const changed=()=>{systemDark=query.matches;apply();}; query.addEventListener("change",changed);
    return ()=>query.removeEventListener("change",changed);
  });
</script>
<label class="theme-switch">顯示模式<select aria-label="顯示模式" bind:value={preference} onchange={apply}><option value="system">依系統</option><option value="light">淺色</option><option value="dark">深色</option></select></label>

<label class="theme-switch">文字大小<select aria-label="文字大小" bind:value={textSize} onchange={applySize}><option value="100">100%</option><option value="125">125%</option><option value="150">150%</option><option value="200">200%</option></select></label>
