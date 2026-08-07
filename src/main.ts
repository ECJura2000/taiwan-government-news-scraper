import "./app.css";
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
  getCurrentWindow().setTitle("各機關新聞整理｜介面就緒").catch((cause) => {
    console.error("Failed to publish the rendered-interface ready marker", cause);
  });
}

export default app;
