import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import { previewCleanup } from "./cleanup";
import { testInventory } from "../test-fixtures/inventory";
const api = vi.hoisted(() => ({
  load: vi.fn(),
  mode: vi.fn(),
  locks: vi.fn(),
  setLocks: vi.fn(),
  diagnostics: vi.fn(),
  operations: vi.fn(),
  preview: vi.fn(),
  cancel: vi.fn(),
  execute: vi.fn(),
  receipt: vi.fn(),
  reportResult: vi.fn(),
  worktrees: vi.fn(),
}));
vi.mock("./bridge", () => ({
  loadInventory: api.load,
  loadCleanupMode: api.mode,
  errorText: (e: Error) => e.message,
}));
vi.mock("./nativeCleanup", () => ({
  readConversationLocks: api.locks,
  setConversationLocks: api.setLocks,
  readDiagnostics: api.diagnostics,
  readOperations: api.operations,
  previewNative: api.preview,
  cancelNative: api.cancel,
  executeNative: api.execute,
  readNativeHistory: vi.fn().mockResolvedValue([]),
  readNativeReceipt: api.receipt,
  readDiagnosticResult: api.reportResult,
}));
vi.mock("./worktrees", async (original) => ({
  ...(await original<typeof import("./worktrees")>()),
  inspectWorktrees: api.worktrees,
  worktreeInspectionAvailable: () => true,
}));
import App from "./App";
const emptyReports = {
  files: [],
  totalBytes: 0,
  externalBytes: 0,
  complete: true,
  warnings: [],
};
beforeEach(() => {
  localStorage.setItem("agentbroom.language", "en");
  localStorage.setItem("agentbroom.theme", "light");
  api.worktrees.mockResolvedValue({
    scannedAt: 1700001000,
    repositories: [],
    warnings: [],
  });
  api.load.mockResolvedValue(testInventory());
  api.mode.mockResolvedValue("native");
  api.locks.mockResolvedValue([]);
  api.setLocks.mockImplementation((ids: string[], locked: boolean) =>
    Promise.resolve(locked ? ids : []),
  );
  api.diagnostics.mockResolvedValue(emptyReports);
  api.operations.mockResolvedValue([]);
  api.cancel.mockResolvedValue(undefined);
  api.receipt.mockRejectedValue({ code: "receiptUnavailable" });
  api.reportResult.mockRejectedValue({ code: "receiptUnavailable" });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  localStorage.clear();
});
async function start() {
  render(<App />);
  await screen.findByText("Where the bytes live");
}
function list() {
  fireEvent.click(screen.getByRole("button", { name: "Conversations" }));
}
describe("C layout", () => {
  it("includes crash reports once and exposes the four main pages", async () => {
    const base = testInventory();
    api.load.mockResolvedValue(base);
    api.diagnostics.mockResolvedValue({
      ...emptyReports,
      totalBytes: 4096,
      externalBytes: 2048,
      files: [],
    });
    await start();
    expect(screen.getByText("External crash reports")).toBeVisible();
    expect(screen.getByRole("button", { name: "Execution log" })).toBeVisible();
    expect(
      screen.getByText("Codex file usage").parentElement,
    ).toHaveTextContent("File lengths are measured");
    expect(screen.queryByText("Protected")).not.toBeInTheDocument();
  });
  it("switches light/dark without rescanning and remembers the choice", async () => {
    await start();
    fireEvent.click(
      screen.getByRole("button", { name: "Switch to dark mode" }),
    );
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(localStorage.getItem("agentbroom.theme")).toBe("dark");
    cleanup();
    render(<App />);
    await screen.findByText("Where the bytes live");
    expect(
      screen.getByRole("button", { name: "Switch to light mode" }),
    ).toBeVisible();
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
  });
  it("sorts by time/size and keeps cross-row batch selection", async () => {
    const inv = testInventory();
    inv.threads[0].updatedAt = "2020-01-01";
    inv.threads[1].updatedAt = "2026-10-19";
    inv.threads[0].inclusiveBytes = 100;
    inv.threads[1].inclusiveBytes = 900;
    api.load.mockResolvedValue(inv);
    await start();
    list();
    const body = screen.getByRole("table").querySelector("tbody")!;
    expect(within(body).getAllByRole("row")[0]).toHaveTextContent(
      "Local synthetic 1",
    );
    fireEvent.change(
      screen.getByRole("combobox", { name: "Sort conversations" }),
      { target: { value: "updatedAsc" } },
    );
    expect(within(body).getAllByRole("row")[0]).toHaveTextContent(
      "Local synthetic 0",
    );
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select Local synthetic 0" }),
    );
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select Local synthetic 1" }),
    );
    expect(
      screen.getByRole("button", { name: "Clean selected (2)" }),
    ).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Lock selected" }));
    await waitFor(() =>
      expect(api.setLocks).toHaveBeenCalledWith(
        ["synthetic-0", "synthetic-1"],
        true,
      ),
    );
    expect(
      screen.getByRole("button", { name: "Clean selected (2)" }),
    ).toBeDisabled();
  });
  it("persists row locks and reloads them on rescan", async () => {
    await start();
    list();
    fireEvent.click(
      screen.getByRole("button", { name: "Lock Local synthetic 0" }),
    );
    await screen.findByRole("button", { name: "Unlock Local synthetic 0" });
    api.locks.mockResolvedValue(["synthetic-0"]);
    fireEvent.click(screen.getByRole("button", { name: "Rescan Codex" }));
    await screen.findByRole("button", { name: "Unlock Local synthetic 0" });
    expect(
      screen.getByRole("button", { name: "Clean Local synthetic 0" }),
    ).toBeDisabled();
  });
  it("retains inventory but pauses cleanup when lock state is unknown", async () => {
    api.locks.mockRejectedValue({ code: "lockStateUnavailable" });
    await start();
    list();
    expect(screen.getAllByText("Lock state unknown").length).toBeGreaterThan(0);
    expect(
      screen.getByRole("button", { name: "Clean Local synthetic 0" }),
    ).toBeDisabled();
  });
  it("shows persisted operation outcomes, titles and seven day retention", async () => {
    api.operations.mockResolvedValue([
      {
        id: "op-1",
        at: 1792368000,
        action: "cleanup",
        status: "verified",
        ids: ["deleted-id"],
        titles: ["Deleted synthetic conversation"],
        count: 3,
        bytes: 123,
        error: null,
      },
    ]);
    await start();
    fireEvent.click(screen.getByRole("button", { name: "Execution log" }));
    expect(
      await screen.findByText(/Deleted synthetic conversation/),
    ).toBeVisible();
    expect(screen.getByText("Keep 7 days")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "View details" }));
    await waitFor(() =>
      expect(
        screen.getByRole("dialog", { name: "Conversation cleanup" }),
      ).toHaveTextContent("Observed removed file lengths"),
    );
  });
  it("keeps the browser read-only while allowing theme changes", async () => {
    api.mode.mockResolvedValue("readOnly");
    await start();
    list();
    expect(
      screen.getByRole("button", { name: "Clean Local synthetic 0" }),
    ).toBeDisabled();
    expect(api.diagnostics).not.toHaveBeenCalled();
    expect(api.locks).not.toHaveBeenCalled();
  });
  it("retries source failures without synthetic fallback", async () => {
    api.load.mockRejectedValueOnce(new Error("source unavailable"));
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "source unavailable",
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry inspection" }));
    await screen.findByText("Where the bytes live");
  });
});
it("opens a backend impact preview directly from a row without invoking deletion", async () => {
  api.preview.mockRejectedValue({ code: "invalidSelection" });
  await start();
  list();
  fireEvent.click(
    screen.getByRole("button", { name: "Clean Local synthetic 0" }),
  );
  await waitFor(() =>
    expect(api.preview).toHaveBeenCalledWith(["synthetic-0"], []),
  );
  expect(
    screen.getByRole("dialog", { name: "Delete selected conversations?" }),
  ).toBeVisible();
});
it("does not silently truncate page selection at the batch limit", async () => {
  const inv = testInventory();
  const template = inv.threads[0];
  inv.threads = Array.from({ length: 150 }, (_, i) => ({
    ...template,
    id: `generated-${i.toString().padStart(3, "0")}`,
    title: `Generated ${i.toString().padStart(3, "0")}`,
    updatedAt: "2026-01-01",
    parentId: null,
    rootId: `generated-${i.toString().padStart(3, "0")}`,
  }));
  api.load.mockResolvedValue(inv);
  await start();
  list();
  const select = () =>
    fireEvent.click(screen.getByRole("checkbox", { name: "Select this page" }));
  select();
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  select();
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  select();
  expect(screen.getByRole("alert")).toHaveTextContent(
    "Select up to 128 conversations per cleanup.",
  );
  expect(
    screen.getByRole("button", { name: "Clean selected (100)" }),
  ).toBeVisible();
});
it("keeps saved locks when operation history fails", async () => {
  api.operations.mockRejectedValue({ code: "operationLogUnavailable" });
  await start();
  list();
  fireEvent.click(
    screen.getByRole("button", { name: "Lock Local synthetic 0" }),
  );
  expect(
    await screen.findByRole("button", { name: "Unlock Local synthetic 0" }),
  ).toBeVisible();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Locks saved. Operation history is unavailable.",
  );
});
it("shows actual scan time and distinguishes preview bytes from removed bytes", async () => {
  api.operations.mockResolvedValue([
    {
      id: "preview-generated",
      at: 1792368000,
      action: "preview",
      status: "ready",
      ids: [],
      titles: [],
      count: 1,
      bytes: 256,
      error: null,
    },
  ]);
  await start();
  expect(screen.queryByText("Updated just now")).not.toBeInTheDocument();
  expect(screen.getByText(/^Updated /)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Execution log" }));
  await screen.findByText("Cleanup preview");
  fireEvent.click(screen.getByRole("button", { name: "View details" }));
  expect(screen.getByRole("dialog")).toHaveTextContent("Measured file lengths");
  expect(screen.getByRole("dialog")).not.toHaveTextContent(
    "Observed removed file lengths",
  );
});

it("list delete opens a single confirmation and one explicit click executes, without text or extra checkbox", async () => {
  const inv = testInventory(3);
  api.load.mockResolvedValue(inv);
  const analysis = previewCleanup(inv, ["synthetic-0"], []);
  api.preview.mockResolvedValue({
    planId: "list-confirmation",
    testOnly: false,
    analysis,
    nativeIds: analysis.includedIds,
    retainedIds: ["synthetic-1"],
    blockedReasons: [],
    executable: true,
    expiresInSeconds: 120,
  });
  api.execute.mockResolvedValue({
    status: "verified",
    removed: [],
    remaining: [],
    observedRemovedLogicalBytes: 0,
    freedBytes: null,
  });
  await start();
  list();
  fireEvent.click(
    screen.getByRole("button", { name: "Clean Local synthetic 0" }),
  );
  const button = await screen.findByRole("button", {
    name: "Permanently delete",
  });
  const dialog = screen.getByRole("dialog");
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(within(dialog).queryByRole("checkbox")).not.toBeInTheDocument();
  expect(within(dialog).queryByRole("textbox")).not.toBeInTheDocument();
  expect(
    within(dialog).queryByRole("button", { name: "Continue to delete…" }),
  ).not.toBeInTheDocument();
  expect(api.execute).not.toHaveBeenCalled();
  fireEvent.click(button);
  await screen.findByText("Native cleanup verified");
  expect(api.execute).toHaveBeenCalledExactlyOnceWith(
    "list-confirmation",
    { phrase: "DELETE", acknowledged: true, selectedIds: ["synthetic-0"] },
    [],
  );
});

it("shows batch actions only for a selection and clears them without executing", async () => {
  await start();
  list();
  expect(
    screen.queryByRole("toolbar", { name: "Selected conversations" }),
  ).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Select Local synthetic 0" }),
  );
  expect(
    screen.getByRole("toolbar", { name: "Selected conversations" }),
  ).toHaveTextContent("1 selected");
  fireEvent.click(screen.getByRole("button", { name: "Clear selection" }));
  expect(
    screen.queryByRole("toolbar", { name: "Selected conversations" }),
  ).not.toBeInTheDocument();
  expect(api.setLocks).not.toHaveBeenCalled();
  expect(api.execute).not.toHaveBeenCalled();
});
it("preserves selection across pages and removes the toolbar when the final selection is cleared", async () => {
  api.load.mockResolvedValue(testInventory(60));
  await start();
  list();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Select Local synthetic 0" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  expect(
    screen.getByRole("toolbar", { name: "Selected conversations" }),
  ).toHaveTextContent("1 selected");
  fireEvent.click(screen.getByRole("button", { name: "Clear selection" }));
  fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
  expect(
    screen.getByRole("checkbox", { name: "Select Local synthetic 0" }),
  ).not.toBeChecked();
  expect(
    screen.queryByRole("toolbar", { name: "Selected conversations" }),
  ).not.toBeInTheDocument();
});
it("keeps the Chinese measurement disclosure available without another scan", async () => {
  localStorage.setItem("agentbroom.language", "zh-CN");
  render(<App />);
  const summary = await screen.findByText("计量说明");
  const details = summary.closest("details")!;
  expect(details).not.toHaveAttribute("open");
  fireEvent.click(summary);
  expect(details).toHaveAttribute("open");
  expect(details).toHaveTextContent("不代表清理后可回收的磁盘空间");
  expect(api.load).toHaveBeenCalledOnce();
});

it("keeps scan counts inline and removes redundant scan and single-lock detail actions", async () => {
  api.operations.mockResolvedValue([
    {
      id: "scan-synthetic",
      at: 1792396800,
      action: "scan",
      status: "verified",
      ids: [],
      titles: [],
      count: 1234,
      bytes: 0,
      error: null,
    },
    {
      id: "lock-synthetic",
      at: 1792396800,
      action: "lock",
      status: "verified",
      ids: ["synthetic-0"],
      titles: ["Synthetic lock"],
      count: 1,
      bytes: 0,
      error: null,
    },
  ]);
  await start();
  fireEvent.click(screen.getByRole("button", { name: "Execution log" }));
  await screen.findByText(/1234 scanned files/);
  expect(
    screen.queryByRole("button", { name: "View details" }),
  ).not.toBeInTheDocument();
  expect(api.receipt).not.toHaveBeenCalled();
});

it("opens the independent worktree page when Codex inventory is unavailable", async () => {
  api.load.mockRejectedValue(new Error("source unavailable"));
  render(<App />);
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: "Worktrees" }));
  await screen.findByText(
    "No worktrees found. Enter a repository path and scan again.",
  );
  expect(screen.queryByText("source unavailable")).not.toBeInTheDocument();
  expect(api.worktrees).toHaveBeenCalledOnce();
  expect(api.execute).not.toHaveBeenCalled();
});
