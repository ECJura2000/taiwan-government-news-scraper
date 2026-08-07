import "./app.css";
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

export default app;
