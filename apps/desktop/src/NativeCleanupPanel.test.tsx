import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import { LocaleProvider } from "./i18n";
import { previewCleanup } from "./cleanup";
import vectors from "../../../fixtures/cleanup-cases.json";
import type { Inventory } from "./types";
import NativeCleanupPanel from "./NativeCleanupPanel";
const api = vi.hoisted(() => ({
  preview: vi.fn(),
  execute: vi.fn(),
  receipt: vi.fn(),
  cancel: vi.fn(),
  history: vi.fn(),
}));
vi.mock("./nativeCleanup", () => ({
  previewNative: api.preview,
  executeNative: api.execute,
  cancelNative: api.cancel,
  readNativeReceipt: api.receipt,
  readNativeHistory: api.history,
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
  api.cancel.mockResolvedValue(undefined);
  api.history.mockResolvedValue([]);
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
  fireEvent.click(screen.getByLabelText("Select Synthetic a"));
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
  await screen.findByRole("heading", { name: "Conversations to delete" });
  fireEvent.click(screen.getByRole("button", { name: "Continue to delete…" }));
}
function execute() {
  fireEvent.click(screen.getByRole("button", { name: "Permanently delete" }));
}
describe("native desktop test flow", () => {
  it("gets backend preview and confirms opaque plan once, then refreshes inventory", async () => {
    const refresh = show();
    expect(
      screen.getByRole("button", { name: "Preview impact" }),
    ).toBeDisabled();
    await review();
    expect(api.preview).toHaveBeenCalledWith(["a"], []);
    expect(
      within(screen.getByRole("dialog")).getByText(
        "2 other conversations will be kept.",
      ),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Permanently delete" }),
    ).toBeEnabled();
    execute();
    await screen.findByText("Native synthetic cleanup verified");
    expect(api.execute).toHaveBeenCalledExactlyOnceWith(
      "backend-owned",
      {
        selectedIds: ["a"],
        phrase: "DELETE",
        acknowledged: true,
      },
      [],
    );
    expect(
      screen.getByRole("button", { name: "Permanently delete" }),
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
    fireEvent.click(screen.getByLabelText("Select Synthetic a"));
    fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
    await screen.findByRole("heading", { name: "Conversations to delete" });
    expect(
      screen.getByRole("button", { name: "Continue to delete…" }),
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
    await screen.findByRole("heading", { name: "待删除的对话" });
    fireEvent.click(screen.getByRole("button", { name: "确认删除…" }));
    fireEvent.click(screen.getByRole("button", { name: "永久删除" }));
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
    fireEvent.click(screen.getByLabelText("Select Synthetic a"));
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
    fireEvent.click(screen.getByRole("button", { name: "Permanently delete" }));
    expect(api.execute).toHaveBeenCalledOnce();
    done(receipt);
    await screen.findByText("Native synthetic cleanup verified");
  });
  it("real mode uses real confirmation wording and passes current Keep choices", async () => {
    api.preview.mockResolvedValue({ ...plan(), testOnly: false });
    render(
      <LocaleProvider>
        <NativeCleanupPanel
          inventory={inventory}
          kept={["e"]}
          testOnly={false}
          refresh={() => {}}
        />
      </LocaleProvider>,
    );
    await review();
    expect(
      screen.getByText(
        "These real conversations and their Codex metadata will be permanently deleted. This cannot be undone.",
      ),
    ).toBeVisible();
    execute();
    await screen.findByText("Native cleanup verified");
    expect(api.execute).toHaveBeenCalledWith(
      "backend-owned",
      expect.objectContaining({ selectedIds: ["a"] }),
      ["e"],
    );
    expect(
      screen.queryByText(
        "Official Codex deletion ran only against generated test data.",
      ),
    ).not.toBeInTheDocument();
  });
  it("unknown persisted result is never displayed as success", async () => {
    api.preview.mockResolvedValue({ ...plan(), testOnly: false });
    api.execute.mockResolvedValue({ ...receipt, status: "unknown" });
    render(
      <LocaleProvider>
        <NativeCleanupPanel
          inventory={inventory}
          kept={[]}
          testOnly={false}
          refresh={() => {}}
        />
      </LocaleProvider>,
    );
    await review();
    execute();
    await screen.findByText(
      "Cleanup outcome is unknown. Do not repeat deletion.",
    );
    expect(
      screen.queryByText("Native cleanup verified"),
    ).not.toBeInTheDocument();
  });
  it("saved unknown history can be inspected without invoking deletion", async () => {
    api.history.mockResolvedValue([
      { ...receipt, planId: "saved-op", selectedIds: ["a"], status: "unknown" },
    ]);
    api.receipt.mockResolvedValue({
      ...receipt,
      planId: "saved-op",
      selectedIds: ["a"],
      status: "unknown",
    });
    render(
      <LocaleProvider>
        <NativeCleanupPanel
          inventory={inventory}
          kept={[]}
          testOnly={false}
          refresh={() => {}}
        />
      </LocaleProvider>,
    );
    await screen.findByText("Recent cleanup results (1)");
    fireEvent.click(screen.getByText("Recent cleanup results (1)"));
    fireEvent.click(
      screen.getByRole("button", { name: "Check execution result" }),
    );
    expect(api.receipt).toHaveBeenCalledWith("saved-op");
    expect(api.execute).not.toHaveBeenCalled();
  });
});

it("manual lock disables its candidate and scope lock blocks confirmation", async () => {
  api.preview.mockResolvedValue({
    ...plan(),
    executable: false,
    blockedReasons: ["lockedConversation"],
    lockedIds: ["c"],
  });
  show(["e"]);
  expect(
    screen.getByRole("radio", { name: "Select Synthetic e" }),
  ).toBeDisabled();
  fireEvent.click(screen.getByRole("radio", { name: "Select Synthetic a" }));
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
  await screen.findByText("Locked conversations in this scope");
  expect(
    screen.getByRole("button", { name: "Continue to delete…" }),
  ).toBeDisabled();
  expect(api.execute).not.toHaveBeenCalled();
});
it("native-indexed child and unresolved logical conversations are selectable", async () => {
  const child = {
    ...inventory.threads[0],
    id: "00000000-0000-4000-8000-000000000009",
    relation: "unknown" as const,
    title: "Synthetic orphan",
  };
  render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={{ ...inventory, threads: [child] }}
        kept={[]}
        refresh={() => {}}
      />
    </LocaleProvider>,
  );
  expect(
    screen.getByRole("radio", { name: "Select Synthetic orphan" }),
  ).toBeEnabled();
});

it("locked parent disables its child while an independent fork stays selectable", () => {
  const root = {
    ...inventory.threads[0],
    id: "00000000-0000-4000-8000-000000000001",
    title: "Synthetic root",
  };
  const child = {
    ...root,
    id: "00000000-0000-4000-8000-000000000002",
    parentId: root.id,
    relation: "spawned" as const,
    title: "Synthetic child",
  };
  const fork = {
    ...root,
    id: "00000000-0000-4000-8000-000000000003",
    parentId: root.id,
    relation: "fork" as const,
    title: "Synthetic fork",
  };
  render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={{ ...inventory, threads: [root, child, fork] }}
        kept={[root.id]}
        refresh={() => {}}
      />
    </LocaleProvider>,
  );
  expect(
    screen.getByRole("radio", { name: "Select Synthetic root" }),
  ).toBeDisabled();
  expect(
    screen.getByRole("radio", { name: "Select Synthetic child" }),
  ).toBeDisabled();
  expect(
    screen.getByRole("radio", { name: "Select Synthetic fork" }),
  ).toBeEnabled();
});

it("a lock added after preview is a clear pre-deletion rejection", async () => {
  api.execute.mockRejectedValue({
    code: "lockedConversation",
    executionPhase: "notStarted",
  });
  show();
  await review();
  execute();
  await screen.findByText(
    "This conversation was locked after the preview. Create a new preview to review the locks.",
  );
  expect(api.execute).toHaveBeenCalledOnce();
  expect(api.receipt).not.toHaveBeenCalled();
});
it("shows the detected runtime and storage contract before confirmation", async () => {
  api.preview.mockResolvedValue({
    ...plan(),
    runtimeVersion: "0.163.0-alpha.10.2",
    storageProfile: "state-v61",
  });
  show();
  await review();
  fireEvent.click(screen.getByText("Files and technical details"));
  expect(
    screen.getByText(
      "Native runtime 0.163.0-alpha.10.2 · storage contract state-v61",
    ),
  ).toBeVisible();
  expect(api.execute).not.toHaveBeenCalled();
});
it("explains storage incompatibility in English without calling it a version rejection", async () => {
  api.preview.mockRejectedValue({ code: "nativeSchemaUnsupported" });
  show();
  fireEvent.click(screen.getByLabelText("Select Synthetic a"));
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
  expect(
    await screen.findByText(
      "Codex storage structure is not yet validated; no database upgrade or migration will be performed.",
    ),
  ).toBeVisible();
  expect(api.execute).not.toHaveBeenCalled();
});
it("keeps thousands of retained conversations out of the preview and waits for the confirmation click", async () => {
  const p = plan();
  p.retainedIds = Array.from({ length: 3819 }, (_, i) => `retained-${i}`);
  api.preview.mockResolvedValue(p);
  render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={inventory}
        kept={[]}
        refresh={() => {}}
        initialSelected={["a"]}
        onClose={() => {}}
      />
    </LocaleProvider>,
  );
  expect(
    await screen.findByText("3819 other conversations will be kept."),
  ).toBeVisible();
  const scope = screen.getByRole("list", { name: "Conversations to delete" });
  expect(scope).toHaveTextContent("Synthetic a");
  expect(scope).toHaveTextContent("Synthetic b");
  expect(scope).toHaveTextContent("Related Sub Agent");
  expect(scope).toHaveTextContent("68 B");
  expect(screen.queryByText("Synthetic d")).not.toBeInTheDocument();
  expect(screen.queryByText("retained-0")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Show 50 more" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByText("Files and technical details").closest("details"),
  ).not.toHaveAttribute("open");
  expect(
    screen.queryByRole("button", { name: "Preview impact" }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Continue to delete…" }));
  expect(
    screen.getByRole("button", { name: "Permanently delete" }),
  ).toBeEnabled();
  expect(api.execute).not.toHaveBeenCalled();
});

it("uses the native affected IDs again at final confirmation even when observation IDs differ", async () => {
  const p = plan();
  p.nativeIds = [...p.nativeIds, "backend-extra"];
  api.preview.mockResolvedValue(p);
  show();
  await review();
  const dialog = screen.getByRole("dialog");
  expect(
    within(dialog).getByRole("list", { name: "Conversations to delete" }),
  ).toHaveTextContent("backend-extra");
  expect(dialog).toHaveTextContent("4 observed records");
  expect(api.execute).not.toHaveBeenCalled();
});
it("direct list action opens only one confirmation without typing, clicking once executes once", async () => {
  const close = vi.fn();
  render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={inventory}
        kept={[]}
        refresh={() => {}}
        initialSelected={["a"]}
        testOnly
        directConfirm
        onClose={close}
      />
    </LocaleProvider>,
  );
  await screen.findByRole("button", { name: "Permanently delete" });
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Continue to delete…" }),
  ).not.toBeInTheDocument();
  expect(api.execute).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Permanently delete" }));
  fireEvent.click(screen.getByRole("button", { name: "Permanently delete" }));
  await screen.findByText("Native synthetic cleanup verified");
  expect(api.execute).toHaveBeenCalledExactlyOnceWith(
    "backend-owned",
    { phrase: "DELETE", acknowledged: true, selectedIds: ["a"] },
    [],
  );
});
it("cancelling a loading confirmation immediately dismisses it and cancels the late plan even before unmount", async () => {
  let resolve!: (p: ReturnType<typeof plan>) => void;
  api.preview.mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const close = vi.fn();
  const view = render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={inventory}
        kept={[]}
        refresh={() => {}}
        initialSelected={["a"]}
        testOnly
        directConfirm
        onClose={close}
      />
    </LocaleProvider>,
  );
  expect(
    screen.queryByRole("button", { name: "Permanently delete" }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cancel deletion" }));
  expect(close).toHaveBeenCalledOnce();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  resolve(plan());
  await waitFor(() => expect(api.cancel).toHaveBeenCalledWith("backend-owned"));
  expect(api.execute).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  view.unmount();
});
it("direct blocked plan shows reasons and cannot expose deletion", async () => {
  api.preview.mockResolvedValue({
    ...plan(),
    executable: false,
    blockedReasons: ["lockedConversation"],
  });
  render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={inventory}
        kept={[]}
        refresh={() => {}}
        initialSelected={["a"]}
        testOnly
        directConfirm
        onClose={() => {}}
      />
    </LocaleProvider>,
  );
  expect(await screen.findByRole("alert")).toBeVisible();
  expect(
    screen.getByText(
      "Deletion is unavailable for this selection. Review the reasons below.",
    ),
  ).toBeVisible();
  expect(
    screen.queryByText(
      "Real Codex deletion is locked in this release; native effects are not fully verified.",
    ),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Permanently delete" }),
  ).not.toBeInTheDocument();
  expect(api.execute).not.toHaveBeenCalled();
});

it("cancel after loading dismisses the confirmation and never calls execute", async () => {
  const close = vi.fn();
  render(
    <LocaleProvider>
      <NativeCleanupPanel
        inventory={inventory}
        kept={[]}
        refresh={() => {}}
        initialSelected={["a"]}
        testOnly
        directConfirm
        onClose={close}
      />
    </LocaleProvider>,
  );
  await screen.findByRole("button", { name: "Permanently delete" });
  fireEvent.click(screen.getByRole("button", { name: "Cancel deletion" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(close).toHaveBeenCalledOnce();
  expect(api.cancel).toHaveBeenCalledWith("backend-owned");
  expect(api.execute).not.toHaveBeenCalled();
});

it("preserves the original execution error when receipt lookup fails, with no repeat execution", async () => {
  api.execute.mockRejectedValue({
    code: "sourceChanged",
    message: "sourceChanged",
    executionPhase: "unknown",
  });
  api.receipt.mockRejectedValue({ code: "receiptUnavailable" });
  show();
  await review();
  execute();
  await screen.findByText("Execution error: sourceChanged");
  fireEvent.click(
    screen.getByRole("button", { name: "Check execution result" }),
  );
  await screen.findByText(
    "No receipt is available yet. Do not repeat deletion.",
  );
  expect(screen.getByText("Execution error: sourceChanged")).toBeVisible();
  expect(
    screen.getByText(
      "Execution outcome is unknown. Do not automatically retry.",
    ),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Permanently delete" }),
  ).toBeDisabled();
  expect(api.execute).toHaveBeenCalledOnce();
});
it.each([
  "sourceChanged",
  "runtimeChanged",
  "relatedStateUnavailable",
  "invalidPlan",
])("uses the owner phase for %s preflight rejection", async (code) => {
  api.execute.mockRejectedValue({
    code,
    message: code,
    executionPhase: "notStarted",
  });
  show();
  await review();
  execute();
  await screen.findByText(
    "Deletion did not start. Close this dialog and create a fresh preview.",
  );
  expect(screen.getByText(`Execution error: ${code}`)).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Check execution result" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Permanently delete" }),
  ).toBeDisabled();
  expect(api.execute).toHaveBeenCalledOnce();
});
it("keeps string IPC failures visible and never infers preflight safety from a code", async () => {
  api.execute.mockRejectedValue("IPC transport disconnected");
  show();
  await review();
  execute();
  await screen.findByText("Execution error: IPC transport disconnected");
  expect(
    screen.getByText(
      "Execution outcome is unknown. Do not automatically retry.",
    ),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Check execution result" }),
  ).toBeVisible();
});
