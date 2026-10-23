import { afterEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: api.invoke,
  isTauri: () => true,
}));
import { inspectWorktrees, type WorktreeInventory } from "./worktrees";
const request = { repository: null, target: null, projects: [] };
const result: WorktreeInventory = {
  scannedAt: 1,
  repositories: [],
  warnings: [],
};
afterEach(() => vi.resetAllMocks());
it("shares identical StrictMode reads and permits a fresh read after completion", async () => {
  let finish!: (value: WorktreeInventory) => void;
  api.invoke.mockImplementation(
    () =>
      new Promise<WorktreeInventory>((r) => {
        finish = r;
      }),
  );
  const first = inspectWorktrees(request),
    second = inspectWorktrees({ ...request });
  expect(first).toBe(second);
  await vi.waitFor(() => expect(api.invoke).toHaveBeenCalledOnce());
  finish(result);
  await first;
  api.invoke.mockResolvedValue(result);
  await inspectWorktrees(request);
  expect(api.invoke).toHaveBeenCalledTimes(2);
});
it("serializes different requests and recovers after the earlier read fails", async () => {
  let fail!: (error: Error) => void;
  api.invoke
    .mockImplementationOnce(
      () =>
        new Promise<WorktreeInventory>((_, r) => {
          fail = r;
        }),
    )
    .mockResolvedValue(result);
  const first = inspectWorktrees(request);
  const rejection = expect(first).rejects.toThrow("failed");
  const next = inspectWorktrees({ ...request, target: "develop" });
  await vi.waitFor(() => expect(api.invoke).toHaveBeenCalledOnce());
  fail(new Error("failed"));
  await rejection;
  await expect(next).resolves.toEqual(result);
  expect(api.invoke).toHaveBeenCalledTimes(2);
});
