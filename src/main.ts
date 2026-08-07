import "./app.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("Missing #app mount target");
}

const app = mount(App, { target });
document.documentElement.dataset.appReady = "true";
document.title = "各機關新聞整理｜介面就緒";
document.getElementById("boot-status")?.remove();
if ("__TAURI_INTERNALS__" in window) {
  invoke("mark_frontend_ready").catch((cause) => {
    console.error("Failed to publish the rendered-interface ready marker", cause);
  });
  getCurrentWindow().setTitle("各機關新聞整理｜介面就緒").catch((cause) => {
    console.error("Failed to update the rendered-interface title", cause);
  });
}

export default app;
