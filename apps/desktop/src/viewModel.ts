import type { Thread } from "./types";
export type SortKey =
  | "updatedDesc"
  | "updatedAsc"
  | "sizeDesc"
  | "sizeAsc"
  | "ownDesc"
  | "title";
export function sortThreads(threads: Thread[], key: SortKey) {
  return [...threads].sort((a, b) => {
    let value = 0;
    if (key.startsWith("updated")) {
      const x = Date.parse(a.updatedAt),
        y = Date.parse(b.updatedAt);
      if (!Number.isFinite(x) || !Number.isFinite(y))
        return Number.isFinite(x)
          ? -1
          : Number.isFinite(y)
            ? 1
            : a.id.localeCompare(b.id);
      value = (x - y) * (key === "updatedAsc" ? 1 : -1);
    } else if (key === "title") value = a.title.localeCompare(b.title);
    else
      value =
        ((key === "ownDesc" ? a.ownBytes : a.inclusiveBytes) -
          (key === "ownDesc" ? b.ownBytes : b.inclusiveBytes)) *
        (key === "sizeAsc" ? 1 : -1);
    return value || a.id.localeCompare(b.id);
  });
}
