import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  LcuStatus,
  OverlayStatePayload,
  RuntimeSummary,
} from "../types/hexbox";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function tauriGetRuntimeSummary(): Promise<RuntimeSummary | null> {
  if (!isTauriRuntime()) return null;
  return invoke<RuntimeSummary>("get_runtime_summary");
}

export async function tauriGetSettings(): Promise<AppSettings | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AppSettings>("get_settings");
}

export async function tauriUpdateSettings(settings: AppSettings): Promise<AppSettings | null> {
  if (!isTauriRuntime()) return null;
  return invoke<AppSettings>("update_settings", { settings });
}

export async function tauriGetOverlayState(): Promise<OverlayStatePayload | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OverlayStatePayload>("get_overlay_state");
}

export async function tauriSetOverlayState(
  payload: OverlayStatePayload
): Promise<OverlayStatePayload | null> {
  if (!isTauriRuntime()) return null;
  return invoke<OverlayStatePayload>("set_overlay_state", { payload });
}

export async function tauriProbeLcuStatus(): Promise<LcuStatus | null> {
  if (!isTauriRuntime()) return null;
  return invoke<LcuStatus>("probe_lcu_status");
}

export async function tauriAcceptReadyCheck(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("accept_ready_check");
  return true;
}

export async function tauriBenchSwap(championId: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("bench_swap", { championId });
  return true;
}

export async function tauriRunOcrPreset(
  presetKey?: string,
  imagePath?: string
): Promise<Record<string, unknown> | null> {
  if (!isTauriRuntime()) return null;
  return invoke<Record<string, unknown>>("run_ocr_preset", {
    presetKey: presetKey ?? null,
    imagePath: imagePath ?? null,
  });
}

export async function tauriShowOverlay(kind: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("show_overlay", { kind });
  return true;
}

export async function tauriHideOverlay(kind: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  await invoke<void>("hide_overlay", { kind });
  return true;
}

export async function tauriToggleOverlay(kind: string): Promise<boolean | null> {
  if (!isTauriRuntime()) return null;
  return invoke<boolean>("toggle_overlay", { kind });
}

export async function subscribeLcuStatus(
  handler: (status: LcuStatus) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  return listen<LcuStatus>("hexbox://lcu-status", (event) => {
    handler(event.payload);
  });
}

export async function subscribeOverlayState(
  handler: (payload: OverlayStatePayload) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  return listen<OverlayStatePayload>("hexbox://overlay-state", (event) => {
    handler(event.payload);
  });
}

export async function subscribeSettingsUpdated(
  handler: (settings: AppSettings) => void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => {};
  return listen<AppSettings>("hexbox://settings-updated", (event) => {
    handler(event.payload);
  });
}
