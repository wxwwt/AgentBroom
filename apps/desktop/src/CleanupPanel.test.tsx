import { afterEach, describe, it, expect, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import vectors from "../../../fixtures/cleanup-cases.json";
import type { Inventory } from "./types";
import { LocaleProvider } from "./i18n";
import CleanupPanel, {
  CleanupConfirmation,
  type CleanupResult,
} from "./CleanupPanel";
import { previewCleanup } from "./cleanup";
afterEach(() => cleanup());
const inventory = () => structuredClone(vectors[0].inventory) as Inventory;
function panel(kept: string[] = []) {
  return render(
    <LocaleProvider>
      <CleanupPanel inventory={inventory()} kept={kept} />
    </LocaleProvider>,
  );
}
function openPreview() {
  fireEvent.click(screen.getByRole("checkbox", { name: "Select Synthetic a" }));
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
}
describe("cleanup preview and confirmation, synthetic inventory only", () => {
  it("starts unselected and shows observed tree, retained records and forks separately", () => {
    panel();
    expect(
      screen.getByRole("button", { name: "Preview impact" }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("checkbox", { name: "Select Synthetic b" }),
    ).not.toBeInTheDocument();
    openPreview();
    expect(
      screen.getByText(/3 observed records · 4 measured files/),
    ).toBeVisible();
    expect(screen.getByText("Synthetic b · b · spawned")).toBeVisible();
    expect(
      screen.getByText("2 other conversations will be kept."),
    ).toBeVisible();
    expect(screen.queryByText("Synthetic d · d")).not.toBeInTheDocument();
    expect(
      screen.getByText(
        "No observed outside dependencies. Native reference coverage remains unverified.",
      ),
    ).toBeVisible();
    expect(screen.getByText("Observed files (4)")).toBeVisible();
  });
  it("lists an external fork only when a reference crosses the selected scope", () => {
    const inv = structuredClone(vectors[2].inventory) as Inventory;
    render(
      <LocaleProvider>
        <CleanupPanel inventory={inv} kept={[]} />
      </LocaleProvider>,
    );
    openPreview();
    const dependencies = screen
      .getByText("Outside dependencies retained")
      .closest("details")!;
    expect(within(dependencies).getByText("Synthetic d · d")).toBeVisible();
    expect(
      screen.getByText(
        "An unselected fork or conversation depends on this scope.",
      ),
    ).toBeVisible();
  });
  it("real confirmation exposes no deletion controls", () => {
    panel();
    openPreview();
    fireEvent.click(
      screen.getByRole("button", { name: "Review confirmation" }),
    );
    const dialog = screen.getByRole("dialog");
    expect(
      within(dialog).getByText(
        "Confirming here will not delete local Codex data.",
      ),
    ).toBeVisible();
    expect(within(dialog).queryByRole("checkbox")).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("textbox")).not.toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Permanently delete" }),
    ).not.toBeInTheDocument();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Close dialog" }),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("selection changes revoke the preview and Keep is explicitly blocked", () => {
    panel(["c"]);
    openPreview();
    expect(
      screen.getByText("A selected tree contains a conversation marked Keep."),
    ).toBeVisible();
    expect(
      screen.getByText("A selected tree contains a conversation marked Keep."),
    ).toBeVisible();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select Synthetic e" }),
    );
    expect(screen.queryByText("Impact preview")).not.toBeInTheDocument();
  });
  it("Chinese confirmation and reasons translate without changing user identities", () => {
    localStorage.setItem("agentbroom.language", "zh-CN");
    panel(["c"]);
    fireEvent.click(screen.getByRole("checkbox", { name: "选择 Synthetic a" }));
    fireEvent.click(screen.getByRole("button", { name: "预览影响" }));
    expect(screen.getByText("选中子树中含有标记为保留的对话。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "查看确认步骤" }));
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "在此确认不会删除本机 Codex 数据。",
    );
    expect(
      screen.queryByRole("button", { name: "永久删除" }),
    ).not.toBeInTheDocument();
  });
  for (const status of [
    "verified",
    "partialFailure",
    "verificationFailed",
  ] as const)
    it(`synthetic-only confirmation preserves the ${status} outcome`, async () => {
      const p = {
        ...previewCleanup(inventory(), ["a"], []),
        sourceKind: "synthetic",
        blockedReasons: [],
      };
      const close = vi.fn();
      const driver = vi.fn().mockResolvedValue({
        status,
        removed: ["a1.jsonl"],
        remaining: status === "verified" ? [] : ["a2.jsonl"],
        observedRemovedLogicalBytes: 31,
        freedBytes: null,
      } satisfies CleanupResult);
      render(
        <LocaleProvider>
          <CleanupConfirmation plan={p} close={close} testDriver={driver} />
        </LocaleProvider>,
      );
      expect(
        screen.getByRole("button", { name: "Permanently delete" }),
      ).toBeEnabled();
      fireEvent.click(
        screen.getByRole("button", { name: "Permanently delete" }),
      );
      await screen.findByText(
        status === "verified"
          ? "Synthetic cleanup verified"
          : status === "partialFailure"
            ? "Cleanup partially failed"
            : "Cleanup verification failed",
      );
      expect(driver).toHaveBeenCalledTimes(1);
      expect(
        screen.getByRole("button", { name: "Permanently delete" }),
      ).toBeDisabled();
      expect(
        screen.getByText(/Removed file lengths, not disk space reclaimed/),
      ).toBeVisible();
      if (status !== "verified")
        expect(
          screen.queryByText("Synthetic cleanup verified"),
        ).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Permanently delete" }),
      ).toBeDisabled();
    });
  it("cancelling never calls the synthetic driver", () => {
    const p = {
      ...previewCleanup(inventory(), ["a"], []),
      sourceKind: "synthetic",
      blockedReasons: [],
    };
    const close = vi.fn(),
      driver = vi.fn();
    render(
      <LocaleProvider>
        <CleanupConfirmation plan={p} close={close} testDriver={driver} />
      </LocaleProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel deletion" }));
    expect(driver).not.toHaveBeenCalled();
    expect(close).toHaveBeenCalledOnce();
  });
  it("an execution exception is unknown, never success", async () => {
    const p = {
      ...previewCleanup(inventory(), ["a"], []),
      sourceKind: "synthetic",
      blockedReasons: [],
    };
    const driver = vi.fn().mockRejectedValue(Error("offline"));
    render(
      <LocaleProvider>
        <CleanupConfirmation plan={p} close={() => {}} testDriver={driver} />
      </LocaleProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Permanently delete" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Execution outcome is unknown. Do not automatically retry.",
    );
    expect(
      screen.queryByText("Synthetic cleanup verified"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Permanently delete" }),
    ).toBeDisabled();
  });
});
