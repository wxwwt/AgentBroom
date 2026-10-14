import { describe, it, expect } from "vitest";
import vectors from "../../../fixtures/cleanup-cases.json";
import results from "../test-fixtures/cleanup-results.json";
import { previewCleanup, selectable } from "./cleanup";
import type { Inventory } from "./types";
describe("display policy versus the shared Rust planner", () => {
  for (const c of vectors)
    it(`matches every field of Rust output for ${c.name}`, () => {
      const p = previewCleanup(c.inventory as Inventory, c.selected, c.kept);
      expect(p).toEqual(results.find((r) => r.name === c.name)!.expected);
      expect(p.canExecute).toBe(false);
      expect(p.executableFiles).toEqual([]);
    });
  it("does not let a child or unknown record become a top-level selection", () => {
    const inv = structuredClone(vectors[0].inventory) as Inventory;
    expect(selectable(inv.threads[1])).toBe(false);
    for (const selection of [[], ["a", "a"], ["missing"], ["b"]])
      expect(() => previewCleanup(inv, selection, [])).toThrow(
        "invalidSelection",
      );
  });
});
