/// <reference types="vite/client" />
import { invoke as nativeInvoke } from "@tauri-apps/api/core";

// Explicit development preview: load public catalog data, never simulate a run.
export const isPreview = import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)
  && new URLSearchParams(window.location.search).get("preview") === "1";

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isPreview) return nativeInvoke<T>(command, args);
  if (command === "list_sources") {
    const catalog = await import("../../src-tauri/resources/sources.json");
    return catalog.default.map(source => source.name) as T;
  }
  if (command === "default_output_dir") return "預設新聞搜集區" as T;
  if (command === "load_topic_policy" || command === "default_topic_policy") {
    const policy = await import("../../examples/topics/ai-ten-topics.json");
    return structuredClone(policy.default) as T;
  }
  throw new Error("此為介面預覽；請在桌面程式中執行此操作。");
}
