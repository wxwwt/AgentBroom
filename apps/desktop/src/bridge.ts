import { invoke, isTauri } from "@tauri-apps/api/core";
import catalog from "./data/catalog.json";
import type { Inventory, Preview } from "./types";
export const nativeRuntime = isTauri();
export const sourceLabel = nativeRuntime
  ? "Native demo scanner"
  : "Browser fixture catalog";
export async function loadInventory(): Promise<Inventory> {
  return nativeRuntime
    ? invoke<Inventory>("scan_demo")
    : structuredClone(catalog.inventory as Inventory);
}
export async function getPreview(
  selectedIds: string[],
  keptIds: string[],
  sourceFingerprint: string,
): Promise<Preview> {
  if (nativeRuntime)
    return invoke("preview_demo", { selectedIds, keptIds, sourceFingerprint });
  if (sourceFingerprint !== catalog.inventory.sourceFingerprint)
    throw {
      code: "staleSnapshot",
      message: "The sample snapshot changed. Rescan.",
    };
  const known = new Set(catalog.inventory.threads.map((t) => t.id));
  for (const ids of [selectedIds, keptIds])
    if (
      ids.length > 32 ||
      new Set(ids).size !== ids.length ||
      ids.some((id) => !known.has(id))
    )
      throw {
        code: "invalidSelection",
        message: "Unknown or duplicate sample identity.",
      };
  if (selectedIds.some((id) => keptIds.includes(id)))
    throw {
      code: "invalidSelection",
      message: "A kept record cannot be reviewed.",
    };
  const key = JSON.stringify([...selectedIds].sort());
  const p = (catalog.previews as Record<string, Preview>)[key];
  if (!p || p.affectedIds.some((id) => keptIds.includes(id)))
    throw {
      code: "invalidSelection",
      message: "Protected sample records cannot be selected.",
    };
  return structuredClone(p);
}
export async function getReport(
  selectedIds: string[],
  keptIds: string[],
  sourceFingerprint: string,
): Promise<string> {
  if (nativeRuntime)
    return invoke("demo_report", { selectedIds, keptIds, sourceFingerprint });
  const preview = await getPreview(selectedIds, keptIds, sourceFingerprint);
  return JSON.stringify(
    {
      schemaVersion: 1,
      sourceKind: "demo",
      executionEnabled: false,
      datasetLabel: catalog.inventory.datasetLabel,
      totals: catalog.inventory.totals,
      warnings: catalog.inventory.warnings,
      preview,
      limitations: [
        "No real agent data inspected",
        "No deletion or database maintenance",
        "Keep choices are session-local",
      ],
    },
    null,
    2,
  );
}
export function errorText(e: unknown): string {
  return e && typeof e === "object" && "message" in e
    ? String(e.message)
    : "The demo could not be loaded. No agent data was accessed.";
}
