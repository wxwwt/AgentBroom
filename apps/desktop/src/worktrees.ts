import { invoke, isTauri } from "@tauri-apps/api/core";
import type { MessageKey } from "./i18n";

export interface Worktree {
  path: string;
  branch: string | null;
  head: string;
  mainCheckout: boolean;
  locked: boolean;
  lockReason: string | null;
  prunable: boolean;
  mergeStatus: "contained" | "notContained" | "unknown";
  aheadCount: number | null;
  lastCommitAt: number | null;
  sourceModifiedAt: number | null;
  modifiedCount: number | null;
  untrackedCount: number | null;
  ignoredCount: number | null;
  logicalBytes: number;
  complete: boolean;
  recommendation: "candidate" | "keep" | "review";
  reasons: string[];
}
export interface WorktreeRepository {
  path: string;
  targetRef: string | null;
  targetOid: string | null;
  shallow: boolean;
  warnings: string[];
  worktrees: Worktree[];
}
export interface WorktreeInventory {
  scannedAt: number;
  repositories: WorktreeRepository[];
  warnings: string[];
}
export interface WorktreeRequest {
  repository: string | null;
  target: string | null;
  projects: string[];
}
export const worktreeInspectionAvailable = () => isTauri();
// Share StrictMode/remount reads and serialize different requests against the native scan owner.
const pending = new Map<string, Promise<WorktreeInventory>>();
let tail: Promise<unknown> = Promise.resolve();
export function inspectWorktrees(
  request: WorktreeRequest,
): Promise<WorktreeInventory> {
  const key = JSON.stringify(request);
  const existing = pending.get(key);
  if (existing) return existing;
  const read = tail
    .catch(() => undefined)
    .then(() => invoke<WorktreeInventory>("inspect_worktrees", { request }));
  const promise = read.finally(() => {
    pending.delete(key);
  });
  pending.set(key, promise);
  tail = promise;
  return promise;
}
export const worktreeMessages: Record<string, MessageKey> = {
  worktreeGitUnavailable: "Git inspection failed or Git is unavailable.",
  worktreeScanLimited: "Scan limit reached. Some results are incomplete.",
  worktreeScanIncomplete: "Some files or Git state could not be inspected.",
  worktreeRepositoryUnavailable:
    "The repository path is unavailable or is not a Git repository.",
  worktreePathEncoding:
    "Git returned a path encoding that cannot be displayed safely.",
  worktreeTargetUnavailable:
    "Choose an existing target branch to check containment.",
  worktreeChangedDuringScan:
    "Git references changed during the scan. Scan again.",
  worktreeMainCheckout: "Main checkout: keep this directory.",
  worktreeLocked: "This worktree is locked in Git.",
  worktreeMissing: "The registered directory is missing or unavailable.",
  worktreeBare: "Bare repository metadata is retained.",
  worktreeMergeUnknown:
    "Containment is unverified; squash or cherry-pick may require manual review.",
  worktreeNotContained:
    "The target branch does not contain the current HEAD. Review unmerged commits.",
  worktreeStatusUnavailable: "Working directory status is unavailable.",
  worktreeIndexHidden:
    "Git index flags can hide local changes; inspect this directory manually.",
  worktreeDirty: "There are uncommitted changes.",
  worktreeUntracked: "There are untracked files.",
  worktreeIgnored:
    "Ignored entries may contain local docs, secrets or build output. Check before removal.",
  worktreeNestedRepository:
    "A nested repository was found; inspect it separately.",
  worktreeScanBusy: "A worktree scan is already running. Try again shortly.",
  invalidWorktreeRequest: "The inspection request exceeds supported limits.",
  worktreeInspectionUnavailable:
    "Worktree inspection is available in the regular desktop app.",
};
export type WorktreeRow = Worktree & { repository: WorktreeRepository };
export function worktreeRows(
  inventory: WorktreeInventory | null,
): WorktreeRow[] {
  return (
    inventory?.repositories.flatMap((repository) =>
      repository.worktrees.map((row) => ({ ...row, repository })),
    ) ?? []
  );
}
export function sortWorktrees(
  rows: WorktreeRow[],
  sort: string,
): WorktreeRow[] {
  return [...rows].sort((a, b) => {
    if (sort === "name") return a.path.localeCompare(b.path);
    const key =
      sort === "size"
        ? "logicalBytes"
        : sort === "commit"
          ? "lastCommitAt"
          : "sourceModifiedAt";
    const av = a[key],
      bv = b[key];
    if (av === null || bv === null)
      return av === bv ? a.path.localeCompare(b.path) : av === null ? 1 : -1;
    return (
      (sort === "oldest" ? av - bv : bv - av) || a.path.localeCompare(b.path)
    );
  });
}
