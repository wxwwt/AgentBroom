import type { Inventory, Thread } from "./types";
/** Display-only projection. This type cannot authorize execution or carry a write token. */
export interface CleanupAnalysis {
  schemaVersion: 1;
  sourceKind: string;
  snapshot: string;
  selectedIds: string[];
  includedIds: string[];
  retainedIds: string[];
  outsideDependents: string[];
  observedFiles: Inventory["items"];
  executableFiles: string[];
  observedLogicalBytes: number;
  blockedReasons: string[];
  completeness: "observedOnly";
  canExecute: false;
}
// Rust BTree ordering is Unicode code point order, not JavaScript UTF-16 order.
function compare(a: string, b: string) {
  const aa = Array.from(a),
    bb = Array.from(b);
  for (let i = 0; i < Math.min(aa.length, bb.length); i++) {
    const d = aa[i].codePointAt(0)! - bb[i].codePointAt(0)!;
    if (d) return d;
  }
  return aa.length - bb.length;
}
const sorted = (s: Iterable<string>) => [...s].sort(compare);
export function selectable(t: Thread) {
  return (t.relation === "root" || t.relation === "fork") && t.files.length > 0;
}
function safePath(p: string) {
  return (
    p.length > 0 &&
    new TextEncoder().encode(p).length <= 2048 &&
    !p.includes("\\") &&
    p.split("/").every((c) => c !== "" && c !== "." && c !== "..")
  );
}
export function previewCleanup(
  inventory: Inventory,
  selected: string[],
  kept: string[],
): CleanupAnalysis {
  if (
    !selected.length ||
    selected.length > 128 ||
    inventory.threads.length > 20_000 ||
    inventory.items.length > 250_000 ||
    new Set(selected).size !== selected.length
  )
    throw Error("invalidSelection");
  const index = new Map<string, Thread>();
  for (const t of inventory.threads) {
    if (!t.id || new TextEncoder().encode(t.id).length > 128 || index.has(t.id))
      throw Error("graphUnresolved");
    index.set(t.id, t);
  }
  for (const id of selected)
    if (!index.has(id) || !selectable(index.get(id)!))
      throw Error("invalidSelection");
  const blocked = new Set(["realStoreLocked", "nativeEffectsUnverified"]);
  if (inventory.partialCoverage) blocked.add("partialCoverage");
  const children = new Map<string, string[]>();
  for (const t of inventory.threads) {
    if (t.relation === "unknown") blocked.add("graphUnresolved");
    if (t.relation === "spawned") {
      if (t.parentId && index.has(t.parentId))
        children.set(t.parentId, [...(children.get(t.parentId) ?? []), t.id]);
      else blocked.add("graphUnresolved");
      const chain = new Set<string>();
      let node: string | null = t.id;
      while (node) {
        if (chain.has(node) || chain.size >= 64) {
          blocked.add("graphUnresolved");
          break;
        }
        chain.add(node);
        const item: Thread | undefined = index.get(node);
        if (!item) {
          blocked.add("graphUnresolved");
          break;
        }
        node = item.relation === "spawned" ? item.parentId : null;
      }
    }
  }
  const included = new Set(selected),
    queue = [...selected];
  while (queue.length)
    for (const child of children.get(queue.pop()!) ?? [])
      if (!included.has(child)) {
        included.add(child);
        queue.push(child);
      }
  for (const id of included) {
    const t = index.get(id)!;
    if (kept.includes(id)) blocked.add("keptRecord");
    if (t.protectionReasons.length) blocked.add("protectedRecord");
    if (t.activity === "active" || t.activity === "unknown")
      blocked.add("activityUnverified");
  }
  const outside = new Set<string>();
  for (const t of inventory.threads) {
    const sources = new Set([
      ...(t.forkSources ?? []),
      ...(t.historySources ?? []),
      ...(t.forkedFromId ? [t.forkedFromId] : []),
      ...(t.sharedHistoryFrom ? [t.sharedHistoryFrom] : []),
      ...(t.relation === "fork" && t.parentId ? [t.parentId] : []),
    ]);
    for (const source of sources) {
      if (!index.has(source)) blocked.add("graphUnresolved");
      if (included.has(source) && !included.has(t.id)) outside.add(t.id);
    }
  }
  if (outside.size) blocked.add("outsideDependency");
  const owners = new Map<string, Set<string>>();
  for (const t of inventory.threads)
    for (const f of t.files) {
      const ids = owners.get(f) ?? new Set<string>();
      ids.add(t.id);
      owners.set(f, ids);
    }
  const measured = new Map<string, Inventory["items"][number]>();
  for (const f of inventory.items) {
    if (measured.has(f.key)) blocked.add("fileOwnershipUnverified");
    measured.set(f.key, f);
  }
  const paths = new Set<string>();
  for (const id of included)
    for (const f of index.get(id)!.files) {
      paths.add(f);
      const item = measured.get(f);
      if (
        !safePath(f) ||
        owners.get(f)!.size !== 1 ||
        !item ||
        !["sessions", "archives"].includes(item.category)
      )
        blocked.add("fileOwnershipUnverified");
    }
  const observedFiles = sorted(paths).flatMap((f) =>
    measured.has(f) ? [measured.get(f)!] : [],
  );
  const observedLogicalBytes = observedFiles.reduce(
    (n, f) => n + f.logicalBytes,
    0,
  );
  if (!Number.isSafeInteger(observedLogicalBytes) || observedLogicalBytes < 0)
    throw Error("fileOwnershipUnverified");
  return {
    schemaVersion: 1,
    sourceKind: inventory.sourceKind,
    snapshot: inventory.sourceFingerprint,
    selectedIds: sorted(selected),
    includedIds: sorted(included),
    retainedIds: sorted(new Set([...included, ...outside])),
    outsideDependents: sorted(outside),
    observedFiles,
    executableFiles: [],
    observedLogicalBytes,
    blockedReasons: sorted(blocked),
    completeness: "observedOnly",
    canExecute: false,
  };
}
export const cleanupReasons = {
  realStoreLocked:
    "Real Codex deletion is locked in this release; native effects are not fully verified.",
  nativeEffectsUnverified:
    "Native database, asset and cascade effects are not fully verified.",
  partialCoverage:
    "Incomplete coverage cannot prove there are no external dependencies.",
  keptRecord: "A selected tree contains a conversation marked Keep.",
  protectedRecord: "A selected tree contains protected records.",
  activityUnverified: "Active or unknown activity prevents cleanup.",
  graphUnresolved: "Unknown, cyclic or missing relationships prevent cleanup.",
  outsideDependency:
    "An unselected fork or conversation depends on this scope.",
  fileOwnershipUnverified:
    "File ownership or measurement is incomplete or conflicting.",
  invalidSelection:
    "Select existing main or independent fork conversations only.",
} as const;
