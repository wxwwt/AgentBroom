import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LocaleProvider } from "./i18n";
import { previewCleanup } from "./cleanup";
import vectors from "../../../fixtures/cleanup-cases.json";
import type { Inventory } from "./types";
import NativeCleanupPanel from "./NativeCleanupPanel";
const api = vi.hoisted(() => ({
  preview: vi.fn(),
  execute: vi.fn(),
  receipt: vi.fn(),
}));
vi.mock("./nativeCleanup", () => ({
  previewNative: api.preview,
  executeNative: api.execute,
  readNativeReceipt: api.receipt,
}));
const inventory = structuredClone(vectors[0].inventory) as Inventory;
function plan() {
  return {
    planId: "backend-owned",
    testOnly: true,
    analysis: previewCleanup(inventory, ["a"], []),
    nativeIds: ["a", "b", "c"],
    retainedIds: ["d", "e"],
    blockedReasons: [],
    executable: true,
    expiresInSeconds: 120,
  };
}
const receipt = {
  status: "verified",
  removed: ["a1.jsonl"],
  remaining: [],
  observedRemovedLogicalBytes: 31,
  freedBytes: null,
};
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
beforeEach(() => {
  localStorage.setItem("agentbroom.language", "en");
  api.preview.mockResolvedValue(plan());
  api.execute.mockResolvedValue(receipt);
  api.receipt.mockResolvedValue(receipt);
});
function show(kept: string[] = []) {
  const refresh = vi.fn();
  render(
    <LocaleProvider>
      <NativeCleanupPanel inventory={inventory} kept={kept} refresh={refresh} />
    </LocaleProvider>,
  );
  return refresh;
}
async function review() {
  fireEvent.click(screen.getByRole("radio", { name: "Select Synthetic a" }));
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
  await screen.findByText("Impact preview");
  fireEvent.click(screen.getByRole("button", { name: "Review confirmation" }));
}
function execute() {
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "DELETE" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Confirm execution" }));
}
describe("native desktop test flow", () => {
  it("gets backend preview and confirms opaque plan once, then refreshes inventory", async () => {
    const refresh = show();
    expect(
      screen.getByRole("button", { name: "Preview impact" }),
    ).toBeDisabled();
    await review();
    expect(api.preview).toHaveBeenCalledWith(["a"], []);
    expect(screen.getByText("Retained conversations")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Confirm execution" }),
    ).toBeDisabled();
    execute();
    await screen.findByText("Native synthetic cleanup verified");
    expect(api.execute).toHaveBeenCalledExactlyOnceWith("backend-owned", {
      selectedIds: ["a"],
      phrase: "DELETE",
      acknowledged: true,
    });
    expect(
      screen.getByRole("button", { name: "Confirm execution" }),
    ).toBeDisabled();
    expect(
      screen.getByText(
        "Official Codex deletion ran only against generated test data.",
      ),
    ).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(refresh).toHaveBeenCalledOnce();
  });
  it("blocked plans cannot expose execution even if observation scope exists", async () => {
    api.preview.mockResolvedValue({
      ...plan(),
      executable: false,
      blockedReasons: ["keptRecord"],
    });
    show(["c"]);
    fireEvent.click(screen.getByRole("radio", { name: "Select Synthetic a" }));
    fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
    await screen.findByText("Impact preview");
    expect(
      screen.getByRole("button", { name: "Review confirmation" }),
    ).toBeDisabled();
    expect(api.execute).not.toHaveBeenCalled();
    expect(api.preview).toHaveBeenCalledWith(["a"], ["c"]);
  });
  it("unknown execution can only query the receipt, never repeat deletion", async () => {
    api.execute.mockRejectedValue(Error("response lost"));
    show();
    await review();
    execute();
    await screen.findByText(
      "Execution outcome is unknown. Do not automatically retry.",
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Check execution result" }),
    );
    await screen.findByText("Native synthetic cleanup verified");
    expect(api.receipt).toHaveBeenCalledExactlyOnceWith("backend-owned");
    expect(api.execute).toHaveBeenCalledOnce();
  });
  it("Chinese preview and native verified state translate", async () => {
    localStorage.setItem("agentbroom.language", "zh-CN");
    show();
    expect(screen.getByText("原生清理合成测试")).toBeVisible();
    fireEvent.click(screen.getByRole("radio", { name: "选择 Synthetic a" }));
    fireEvent.click(screen.getByRole("button", { name: "预览影响" }));
    await screen.findByText("影响预览");
    fireEvent.click(screen.getByRole("button", { name: "查看确认步骤" }));
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "DELETE" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认执行" }));
    await screen.findByText("原生合成清理验证通过");
  });
  for (const status of ["partialFailure", "verificationFailed"] as const)
    it(`native ${status} never claims verified deletion`, async () => {
      api.execute.mockResolvedValue({
        ...receipt,
        status,
        removed: [],
        remaining: ["a1.jsonl"],
      });
      show();
      await review();
      execute();
      await screen.findByText(
        status === "partialFailure"
          ? "Cleanup partially failed"
          : "Cleanup verification failed",
      );
      expect(
        screen.queryByText(
          "Official Codex deletion ran only against generated test data.",
        ),
      ).not.toBeInTheDocument();
      expect(
        screen.getByText(
          "Native deletion was not fully verified. Inspect the result before closing.",
        ),
      ).toBeVisible();
    });
  it("stopped native runtime disables retry and explains restart", async () => {
    api.preview.mockRejectedValue({ code: "runtimeStopped" });
    show();
    fireEvent.click(screen.getByRole("radio", { name: "Select Synthetic a" }));
    fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
    await screen.findByText(
      "The test runtime is stopped. Restart the test app to generate a new store.",
    );
    expect(
      screen.getByRole("button", { name: "Preview impact" }),
    ).toBeDisabled();
  });
  it("confirmation sends once even when clicked twice before completion", async () => {
    let done!: (value: typeof receipt) => void;
    api.execute.mockImplementation(
      () =>
        new Promise((resolve) => {
          done = resolve;
        }),
    );
    show();
    await review();
    execute();
    fireEvent.click(screen.getByRole("button", { name: "Confirm execution" }));
    expect(api.execute).toHaveBeenCalledOnce();
    done(receipt);
    await screen.findByText("Native synthetic cleanup verified");
  });
});
