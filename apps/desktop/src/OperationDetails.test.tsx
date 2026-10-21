import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import { LocaleProvider } from "./i18n";
import type { NativeReceipt, OperationEvent } from "./nativeCleanup";
const api = vi.hoisted(() => ({ receipt: vi.fn(), report: vi.fn() }));
vi.mock("./nativeCleanup", () => ({
  readNativeReceipt: api.receipt,
  readDiagnosticResult: api.report,
}));
import OperationDetails, { operationHasDetails } from "./OperationDetails";
const event: OperationEvent = {
  id: "synthetic-receipt",
  at: 1792396800,
  action: "cleanup",
  status: "partialFailure",
  ids: ["synthetic-main"],
  titles: ["Deleted synthetic conversation"],
  count: 1,
  bytes: 4096,
  error: "retainedFileChangedOrMissing",
};
const receipt: NativeReceipt = {
  planId: event.id,
  status: "partialFailure",
  selectedIds: event.ids,
  expectedIds: ["synthetic-main", "synthetic-child"],
  testOnly: false,
  nativeDeletedIds: ["synthetic-main", "synthetic-child"],
  removed: ["sessions/synthetic-main.jsonl"],
  remaining: ["sessions/synthetic-child.jsonl"],
  observedRemovedLogicalBytes: 4096,
  freedBytes: null,
  error: "filesUnverified",
  warnings: ["Synthetic warning"],
  concurrentChanges: ["sessions/synthetic-keep.jsonl"],
  nativeNotificationsVerified: true,
  filesVerified: false,
  stateVerified: true,
  relatedVerified: false,
};
beforeEach(() => {
  localStorage.setItem("agentbroom.language", "en");
  api.receipt.mockResolvedValue(structuredClone(receipt));
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  localStorage.clear();
});
function show(e = event) {
  return render(
    <LocaleProvider>
      <OperationDetails event={e} records={new Map()} close={() => {}} />
    </LocaleProvider>,
  );
}
it("offers details only when saved information adds something", () => {
  const simple = { ...event, error: null, action: "scan", ids: [], bytes: 0 };
  expect(operationHasDetails(simple)).toBe(false);
  expect(operationHasDetails({ ...simple, action: "lock", ids: ["one"] })).toBe(
    false,
  );
  expect(
    operationHasDetails({ ...simple, action: "lock", ids: ["one", "two"] }),
  ).toBe(true);
  expect(operationHasDetails(event)).toBe(true);
});
it("reads the exact saved receipt and shows files, scope, verification and recorded errors", async () => {
  show();
  await screen.findByText("Verification results");
  expect(api.receipt).toHaveBeenCalledExactlyOnceWith(event.id);
  expect(screen.getByText("Deleted synthetic conversation")).toBeVisible();
  expect(
    screen.getByText("synthetic-child", { selector: "strong" }),
  ).toBeVisible();
  const fileCheck = screen.getByText("File verification").parentElement!;
  expect(fileCheck).toHaveTextContent("Not verified");
  expect(
    screen.getByText("Native deletion notifications").parentElement!,
  ).toHaveTextContent("Passed");
  fireEvent.click(screen.getByText("Removed files (1)"));
  expect(screen.getByText("sessions/synthetic-main.jsonl")).toBeVisible();
  fireEvent.click(screen.getByText("Remaining files (1)"));
  expect(screen.getByText("sessions/synthetic-child.jsonl")).toBeVisible();
  expect(screen.getByText("Synthetic warning")).toBeVisible();
  expect(screen.getByText("sessions/synthetic-keep.jsonl")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("filesUnverified");
  expect(screen.getByRole("alert")).toHaveTextContent(
    "retainedFileChangedOrMissing",
  );
  expect(
    screen.queryByRole("button", { name: "Permanently delete" }),
  ).not.toBeInTheDocument();
});
it("shows a blocked operation's reason without trying to read a receipt", async () => {
  show({ ...event, status: "blocked", error: "sourceChanged", bytes: 0 });
  expect(screen.getByText("Deletion did not start.")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("sourceChanged");
  expect(api.receipt).not.toHaveBeenCalled();
  expect(screen.queryByText("Verification results")).not.toBeInTheDocument();
});
it("keeps log information when receipt is missing and only retries reading", async () => {
  api.receipt.mockRejectedValueOnce({ code: "receiptUnavailable" });
  show();
  await screen.findByText(
    "The saved execution receipt could not be read. Only the operation log is shown.",
  );
  expect(screen.getByText("Deleted synthetic conversation")).toBeVisible();
  fireEvent.click(screen.getByText("Error codes"));
  expect(
    screen.getByText("retainedFileChangedOrMissing", { selector: "code" }),
  ).toBeVisible();
  fireEvent.click(
    screen.getByRole("button", { name: "Retry reading receipt" }),
  );
  await screen.findByText("Verification results");
  expect(api.receipt).toHaveBeenCalledTimes(2);
});
it("does not invent a missing historical verification field", async () => {
  api.receipt.mockResolvedValue({
    ...receipt,
    relatedVerified: undefined,
    expectedIds: undefined,
  });
  show();
  await screen.findByText("Verification results");
  expect(
    screen.getByText("Associated data verification").parentElement!,
  ).toHaveTextContent("Not recorded");
  expect(
    screen.queryByText("synthetic-child", { selector: "strong" }),
  ).not.toBeInTheDocument();
});
it("ignores an old response after switching the operation", async () => {
  let resolve!: (r: NativeReceipt) => void;
  api.receipt.mockReturnValueOnce(
    new Promise<NativeReceipt>((r) => {
      resolve = r;
    }),
  );
  const view = show();
  view.rerender(
    <LocaleProvider>
      <OperationDetails
        event={{
          ...event,
          id: "synthetic-other",
          action: "lock",
          ids: ["one", "two"],
          titles: [],
          bytes: 0,
          error: null,
        }}
        records={new Map()}
        close={() => {}}
      />
    </LocaleProvider>,
  );
  resolve(receipt);
  await waitFor(() =>
    expect(screen.getByRole("dialog")).toHaveTextContent("two"),
  );
  expect(screen.queryByText("Verification results")).not.toBeInTheDocument();
});
it("uses the report receipt route for crash cleanup and handles unknown outcomes", async () => {
  api.report.mockResolvedValue({
    planId: event.id,
    status: "unknown",
    removed: [],
    remaining: ["synthetic.ips"],
    observedRemovedLogicalBytes: 0,
    error: null,
  });
  show({ ...event, action: "crashCleanup", error: null });
  await screen.findByText(
    "Cleanup outcome is unknown. Do not repeat deletion.",
  );
  expect(api.report).toHaveBeenCalledExactlyOnceWith(event.id);
  expect(api.receipt).not.toHaveBeenCalled();
  expect(screen.queryByText("Verification results")).not.toBeInTheDocument();
});
it("translates receipt details and preserves literal file names", async () => {
  localStorage.setItem("agentbroom.language", "zh-CN");
  show();
  await screen.findByText("验证结果");
  expect(screen.getByText("关联数据验证").parentElement!).toHaveTextContent(
    "未验证通过",
  );
  fireEvent.click(screen.getByText("已移除文件 (1)"));
  expect(screen.getByText("sessions/synthetic-main.jsonl")).toBeVisible();
});
it("rejects a mismatched receipt rather than displaying another operation's result", async () => {
  api.receipt.mockResolvedValue({ ...receipt, planId: "wrong-operation" });
  show();
  await screen.findByText(
    "The saved execution receipt could not be read. Only the operation log is shown.",
  );
  expect(screen.queryByText("Verification results")).not.toBeInTheDocument();
});
