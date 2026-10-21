import { invoke } from "@tauri-apps/api/core";
import type { CleanupAnalysis } from "./cleanup";
import type { CleanupResult } from "./CleanupPanel";
export interface NativePlan {
  planId: string;
  testOnly: boolean;
  runtimeVersion?: string;
  storageProfile?: string;
  analysis: CleanupAnalysis;
  nativeIds: string[];
  retainedIds: string[];
  blockedReasons: string[];
  warnings?: string[];
  lockedIds?: string[];
  executable: boolean;
  expiresInSeconds: number;
}
export interface NativeReceipt extends CleanupResult {
  planId: string;
  selectedIds: string[];
  expectedIds?: string[];
  testOnly: boolean;
  nativeDeletedIds: string[];
  concurrentChanges: string[];
  nativeNotificationsVerified: boolean;
  filesVerified: boolean;
  stateVerified: boolean;
  relatedVerified?: boolean;
}
export const previewNative = (selectedIds: string[], kept: string[]) =>
  invoke<NativePlan>("cleanup_preview", { selectedIds, kept });
export const cancelNative = (planId: string) =>
  invoke<void>("cleanup_cancel", { planId });
/** Legacy wire markers are supplied only after the explicit confirmation button click. */
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

export const readConversationLocks = () =>
  invoke<string[]>("conversation_locks");
export const setConversationLock = (threadId: string, locked: boolean) =>
  invoke<string[]>("set_conversation_lock", { threadId, locked });

export interface OperationEvent {
  id: string;
  at: number;
  action: string;
  status: string;
  ids: string[];
  titles: string[];
  count: number;
  bytes: number;
  error: string | null;
}
export interface CrashReport {
  key: string;
  name: string;
  location: string;
  bytes: number;
  modifiedAt: number;
  external: boolean;
}
export interface DiagnosticInventory {
  files: CrashReport[];
  totalBytes: number;
  externalBytes: number;
  externalAllocatedBytes?: number;
  complete: boolean;
  warnings: string[];
}
export interface DiagnosticPlan {
  planId: string;
  files: CrashReport[];
  bytes: number;
  expiresInSeconds: number;
}
export interface DiagnosticResult {
  planId: string;
  status: string;
  removed: string[];
  remaining: string[];
  observedRemovedLogicalBytes: number;
  warnings?: string[];
  error: string | null;
}
export const readOperations = () =>
  invoke<OperationEvent[]>("operation_history");
export const setConversationLocks = (threadIds: string[], locked: boolean) =>
  invoke<string[]>("set_conversation_locks", { threadIds, locked });
export const readDiagnostics = () =>
  invoke<DiagnosticInventory>("diagnostic_inventory");
export const previewDiagnostics = (keys: string[]) =>
  invoke<DiagnosticPlan>("diagnostic_preview", { keys });
export const executeDiagnostics = (
  planId: string,
  phrase: string,
  acknowledged: boolean,
) =>
  invoke<DiagnosticResult>("diagnostic_execute", {
    planId,
    phrase,
    acknowledged,
  });
export const readDiagnosticResult = (planId: string) =>
  invoke<DiagnosticResult>("diagnostic_result", { planId });
export const cancelDiagnostics = (planId: string) =>
  invoke<void>("diagnostic_cancel", { planId });
