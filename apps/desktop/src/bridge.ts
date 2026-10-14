import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Inventory } from "./types";
export const nativeRuntime = isTauri();
export const sourceLabel = nativeRuntime ? "Native Codex scanner" : "Local browser read-only scanner";
let pending: Promise<Inventory> | null = null;
/** Share an in-flight read: React StrictMode must not launch competing native scans. */
export function loadInventory(): Promise<Inventory> {
  if (!pending) {
    pending = readInventory().finally(() => {pending = null;});
  }
  return pending;
}
async function readInventory(): Promise<Inventory> {
  const inventory = nativeRuntime
    ? await invoke<Inventory>("scan_codex")
    : await readLocalBrowser();
  if (inventory.sourceKind !== "codex" || inventory.executionEnabled !== false)
    throw new Error("Only a real read-only Codex inventory is accepted.");
  return inventory;
}
async function readLocalBrowser(): Promise<Inventory> {
  if (!['127.0.0.1','localhost'].includes(location.hostname))
    throw new Error("Local Codex inspection requires the desktop app or a loopback development server.");
  const response = await fetch('/__agentbroom/inventory', {
    headers:{'X-AgentBroom-ReadOnly':'1'}, cache:'no-store', credentials:'same-origin',
  });
  if (!response.ok) {
    const error = await response.json().catch(()=>null);
    throw new Error(typeof error?.message === 'string' ? error.message : "Local inspection failed.");
  }
  return response.json();
}
export function errorText(e: unknown): string {
  return e && typeof e === "object" && "message" in e ? String(e.message) : "Local inspection failed.";
}
