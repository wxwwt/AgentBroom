import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LocaleProvider } from "./i18n";
const api = vi.hoisted(() => ({
  preview: vi.fn(),
  execute: vi.fn(),
  result: vi.fn(),
  cancel: vi.fn(),
}));
vi.mock("./nativeCleanup", () => ({
  previewDiagnostics: api.preview,
  executeDiagnostics: api.execute,
  readDiagnosticResult: api.result,
  cancelDiagnostics: api.cancel,
}));
import DiagnosticPanel from "./DiagnosticPanel";
const file = {
  key: "opaque-one",
  name: "codex-synthetic.ips",
  location: "User diagnostic reports",
  bytes: 123,
  modifiedAt: 1,
  external: true,
};
const inventory = {
  files: [file],
  totalBytes: 123,
  externalBytes: 123,
  complete: true,
  warnings: [],
};
beforeEach(() => {
  localStorage.setItem("agentbroom.language", "en");
  api.preview.mockResolvedValue({
    planId: "reports-synthetic",
    files: [file],
    bytes: 123,
    expiresInSeconds: 120,
  });
  api.execute.mockResolvedValue({
    planId: "reports-synthetic",
    status: "verified",
    removed: [file.key],
    remaining: [],
    observedRemovedLogicalBytes: 123,
    error: null,
  });
  api.cancel.mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
async function preview() {
  render(
    <LocaleProvider>
      <DiagnosticPanel
        inventory={inventory}
        close={() => {}}
        refresh={() => {}}
      />
    </LocaleProvider>,
  );
  expect(screen.getByRole("button", { name: "Preview impact" })).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
  await screen.findByText("Review selected crash reports");
}
async function confirm() {
  await preview();
  expect(api.execute).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "DELETE" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Confirm execution" }));
}
it("previews exact report keys and requires explicit confirmation before removal", async () => {
  await confirm();
  await screen.findByText("Crash report cleanup verified");
  expect(api.preview).toHaveBeenCalledWith([file.key]);
  expect(api.execute).toHaveBeenCalledExactlyOnceWith(
    "reports-synthetic",
    "DELETE",
    true,
  );
});
it("lost response queries its receipt instead of issuing another delete", async () => {
  api.execute.mockRejectedValue(Error("lost reply"));
  api.result.mockResolvedValue({
    status: "verified",
    removed: [file.key],
    remaining: [],
    observedRemovedLogicalBytes: 123,
  });
  await confirm();
  await screen.findByText(
    "Execution outcome is unknown. Do not automatically retry.",
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Check execution result" }),
  );
  await screen.findByText("Crash report cleanup verified");
  expect(api.execute).toHaveBeenCalledOnce();
  expect(api.result).toHaveBeenCalledWith("reports-synthetic");
});
it("partial report failures are not displayed as verified", async () => {
  api.execute.mockResolvedValue({
    status: "partialFailure",
    removed: [],
    remaining: [file.key],
    error: "reportDeleteFailed",
  });
  await confirm();
  await screen.findByText("Cleanup partially failed");
  expect(
    screen.queryByText("Crash report cleanup verified"),
  ).not.toBeInTheDocument();
});
it("known preflight rejection offers a fresh preview and requires a new confirmation", async () => {
  api.execute.mockRejectedValue({ code: "planExpired" });
  await confirm();
  await screen.findByText(
    "Nothing was deleted. Create a fresh preview before confirming again.",
  );
  expect(
    screen.queryByRole("button", { name: "Check execution result" }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Create fresh preview" }));
  await screen.findByRole("button", { name: "Confirm execution" });
  await screen.findByRole("textbox");
  expect(screen.getByRole("textbox")).toHaveValue("");
  expect(screen.getByRole("checkbox")).not.toBeChecked();
  expect(
    screen.getByRole("button", { name: "Confirm execution" }),
  ).toBeDisabled();
  expect(api.execute).toHaveBeenCalledOnce();
  expect(api.preview).toHaveBeenCalledTimes(2);
});
