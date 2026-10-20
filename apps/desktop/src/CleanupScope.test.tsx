import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import CleanupScope from "./CleanupScope";
import { LocaleProvider } from "./i18n";
import vectors from "../../../fixtures/cleanup-cases.json";
import type { Inventory } from "./types";
afterEach(cleanup);
it("uses only backend affected IDs and measured own files, never inclusive totals", () => {
  localStorage.setItem("agentbroom.language", "en");
  const inv = structuredClone(vectors[0].inventory) as Inventory;
  inv.threads[0].inclusiveBytes = 999999;
  inv.threads[0].files.push(inv.threads[0].files[0]);
  render(
    <LocaleProvider>
      <CleanupScope
        ids={["a", "b", "missing"]}
        selectedIds={["a"]}
        records={new Map(inv.threads.map((t) => [t.id, t]))}
        files={inv.items}
      />
    </LocaleProvider>,
  );
  expect(
    screen.getByRole("list", { name: "Conversations to delete" }),
  ).toHaveAttribute("tabindex", "0");
  expect(screen.getByText("68 B")).toBeVisible();
  expect(screen.getByText("43 B")).toBeVisible();
  expect(screen.getByText("missing")).toBeVisible();
  expect(screen.getByText("Unknown")).toBeVisible();
  expect(screen.queryByText("Synthetic d")).not.toBeInTheDocument();
});
