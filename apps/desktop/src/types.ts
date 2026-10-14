export type Relation = "root" | "spawned" | "fork" | "unknown";
export interface Thread {
  id: string;
  title: string;
  project: string;
  relation: Relation;
  activity: "completed" | "active" | "archived" | "unknown";
  updatedAt: string;
  parentId: string | null;
  sharedHistoryFrom: string | null;
  files: string[];
  segmentIds: string[];
  ownBytes: number;
  inclusiveBytes: number;
  ownedDescendantIds: string[];
  protectionReasons: string[];
  sourceType?: string;
  forkedFromId?: string | null;
  forkSources?: string[];
  historySources?: string[];
  relationEvidence?: string[];
}
export interface Total {
  category: string;
  logicalBytes: number;
  allocatedBytes: number | null;
  files: number;
}
export interface Inventory {
  schemaVersion: number;
  sourceKind: "demo" | "codex";
  datasetLabel: string;
  sourceFingerprint: string;
  items: {
    key: string;
    category: string;
    logicalBytes: number;
    allocatedBytes: number | null;
  }[];
  threads: Thread[];
  totals: Total[];
  warnings: string[];
  executionEnabled: false;
  partialCoverage?: boolean;
}
import type {MessageKey, MessageValues} from "./i18n";
export interface ActivityEntry {
  id: number;
  title: MessageKey;
  description: MessageKey;
  values?: MessageValues;
  time: string;
}
export function bytes(n: number | null): string {
  if (n === null) return "Unknown";
  if (n < 1024) return `${n} B`;
  const unit = Math.min(3, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / 1024 ** unit).toFixed(1)} ${["B", "KiB", "MiB", "GiB"][unit]}`;
}
