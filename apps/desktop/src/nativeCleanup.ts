import { invoke } from "@tauri-apps/api/core";
import type { CleanupAnalysis } from "./cleanup";
import type { CleanupResult } from "./CleanupPanel";
export interface NativePlan {
  planId: string;
  testOnly: boolean;
  analysis: CleanupAnalysis;
  nativeIds: string[];
  retainedIds: string[];
  blockedReasons: string[];
  executable: boolean;
  expiresInSeconds: number;
}
export interface NativeReceipt extends CleanupResult {
  planId: string;
  selectedIds: string[];
  testOnly: boolean;
  nativeDeletedIds: string[];
  concurrentChanges: string[];
  nativeNotificationsVerified: boolean;
  filesVerified: boolean;
  stateVerified: boolean;
}
export const previewNative = (selectedIds: string[], kept: string[]) =>
  invoke<NativePlan>("cleanup_preview", { selectedIds, kept });
export const cancelNative = (planId: string) =>
  invoke<void>("cleanup_cancel", { planId });
export const executeNative = (
  planId: string,
  confirmation: {
    phrase: string;
    acknowledged: boolean;
    selectedIds: string[];
  },
  kept: string[] = [],
) =>
  invoke<NativeReceipt>("cleanup_execute", {
    confirmation: { planId, ...confirmation, kept },
  });
export const readNativeReceipt = (planId: string) =>
  invoke<NativeReceipt>("cleanup_result", { planId });

export const readNativeHistory = () =>
  invoke<NativeReceipt[]>("cleanup_history");
