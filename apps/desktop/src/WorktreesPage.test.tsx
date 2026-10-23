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
import WorktreesPage from "./WorktreesPage";
import {
  sortWorktrees,
  worktreeRows,
  type WorktreeInventory,
  type Worktree,
} from "./worktrees";
const api = vi.hoisted(() => ({ inspect: vi.fn(), available: vi.fn() }));
vi.mock("./worktrees", async (original) => ({
  ...(await original<typeof import("./worktrees")>()),
  inspectWorktrees: api.inspect,
  worktreeInspectionAvailable: api.available,
}));
const row = (overrides: Partial<Worktree> = {}): Worktree => ({
  path: "/synthetic/worktrees/merged",
  branch: "feature/merged",
  head: "a".repeat(40),
  mainCheckout: false,
  locked: false,
  lockReason: null,
  prunable: false,
  mergeStatus: "contained",
  aheadCount: 0,
  lastCommitAt: 1577836800,
  sourceModifiedAt: 1700000000,
  modifiedCount: 0,
  untrackedCount: 0,
  ignoredCount: 0,
  logicalBytes: 4096,
  complete: true,
  recommendation: "candidate",
  reasons: [],
  ...overrides,
});
function fixture(): WorktreeInventory {
  return {
    scannedAt: 1700001000,
    warnings: [],
    repositories: [
      {
        path: "/synthetic/project",
        targetRef: "refs/heads/main",
        targetOid: "b".repeat(40),
        shallow: false,
        warnings: [],
        worktrees: [
          row(),
          row({
            path: "/synthetic/worktrees/dirty",
            branch: "feature/dirty",
            mergeStatus: "notContained",
            aheadCount: 2,
            sourceModifiedAt: null,
            modifiedCount: 1,
            untrackedCount: 1,
            ignoredCount: 1,
            logicalBytes: 8192,
            recommendation: "keep",
            reasons: ["worktreeDirty", "worktreeUntracked", "worktreeIgnored"],
          }),
          row({
            path: "/synthetic/worktrees/private",
            branch: "feature/private",
            recommendation: "review",
            ignoredCount: 2,
            logicalBytes: 2000,
            complete: false,
            reasons: ["worktreeIgnored", "worktreeScanIncomplete"],
          }),
        ],
      },
    ],
  };
}
function start() {
  return render(
    <LocaleProvider>
      <WorktreesPage projects={["/synthetic/project"]} />
    </LocaleProvider>,
  );
}
beforeEach(() => {
  localStorage.setItem("agentbroom.language", "en");
  api.available.mockReturnValue(true);
  api.inspect.mockResolvedValue(fixture());
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  localStorage.clear();
});
it("scans read-only and exposes actual containment, changes, timestamps and reasons", async () => {
  start();
  await screen.findByText("feature/merged");
  expect(api.inspect).toHaveBeenCalledExactlyOnceWith({
    repository: null,
    target: null,
    projects: ["/synthetic/project"],
  });
  expect(screen.getByText("Modified 1")).toBeVisible();
  expect(
    screen.queryByRole("button", { name: /delete|remove|clean/i }),
  ).not.toBeInTheDocument();
  const dirty = screen.getByText("feature/dirty").closest("tr")!;
  fireEvent.click(within(dirty).getByText("Reasons and paths"));
  expect(
    within(dirty).getByText("There are uncommitted changes."),
  ).toBeVisible();
  expect(within(dirty).getByText(/Ignored entries may contain/)).toBeVisible();
  expect(within(dirty).getByText("refs/heads/main")).toBeVisible();
  expect(within(dirty).getByText("Unknown")).toBeVisible();
  expect(screen.getByText(/Source modification is an observed/)).toBeVisible();
});
it("filters and sorts while keeping unknown timestamps last", async () => {
  start();
  await screen.findByText("feature/merged");
  fireEvent.change(screen.getByLabelText("Filter worktrees"), {
    target: { value: "candidate" },
  });
  expect(screen.queryByText("feature/dirty")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Filter worktrees"), {
    target: { value: "all" },
  });
  fireEvent.change(screen.getByLabelText("Search worktrees"), {
    target: { value: "private" },
  });
  expect(screen.getByText("feature/private")).toBeVisible();
  expect(screen.queryByText("feature/merged")).not.toBeInTheDocument();
  const rows = worktreeRows(fixture());
  expect(sortWorktrees(rows, "oldest").at(-1)?.sourceModifiedAt).toBeNull();
  expect(sortWorktrees(rows, "size")[0].branch).toBe("feature/dirty");
});
it("uses an explicit repository and target only after submitting, with no stale rows after failure", async () => {
  start();
  await screen.findByText("feature/merged");
  fireEvent.change(screen.getByLabelText("Repository path"), {
    target: { value: "/synthetic/another" },
  });
  fireEvent.change(screen.getByLabelText("Target branch"), {
    target: { value: "develop" },
  });
  expect(api.inspect).toHaveBeenCalledOnce();
  api.inspect.mockRejectedValueOnce({ code: "worktreeRepositoryUnavailable" });
  fireEvent.click(screen.getByRole("button", { name: "Scan worktrees" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The repository path is unavailable",
  );
  expect(screen.queryByText("feature/merged")).not.toBeInTheDocument();
  expect(api.inspect).toHaveBeenLastCalledWith({
    repository: "/synthetic/another",
    target: "develop",
    projects: [],
  });
  fireEvent.click(screen.getByRole("button", { name: "Scan worktrees" }));
  await screen.findByText("feature/merged");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
it("discards late responses after unmount and prevents duplicate button submissions", async () => {
  let resolve!: (value: WorktreeInventory) => void;
  api.inspect.mockReturnValue(
    new Promise<WorktreeInventory>((r) => {
      resolve = r;
    }),
  );
  const view = start();
  expect(
    screen.getByRole("button", { name: "Inspecting worktrees…" }),
  ).toBeDisabled();
  view.unmount();
  resolve(fixture());
  await waitFor(() => expect(api.inspect).toHaveBeenCalledOnce());
});
it("does not invoke native inspection from a browser", () => {
  api.available.mockReturnValue(false);
  start();
  expect(
    screen.getByText(
      "Worktree inspection is available in the regular desktop app.",
    ),
  ).toBeVisible();
  expect(screen.getByRole("button", { name: "Scan worktrees" })).toBeDisabled();
  expect(api.inspect).not.toHaveBeenCalled();
});
it("localizes the page without translating branch names or paths", async () => {
  localStorage.setItem("agentbroom.language", "zh-CN");
  start();
  await screen.findByText("feature/merged");
  expect(screen.getByText("只读")).toBeVisible();
  expect(screen.getByLabelText("目标分支")).toBeVisible();
  expect(screen.getByText("feature/dirty")).toBeVisible();
  expect(screen.getByText("忽略条目 2")).toBeVisible();
});
it("keeps worktree inspection available even when conversation inventory fails", async () => {
  api.inspect.mockResolvedValue({
    scannedAt: 1700001000,
    warnings: [],
    repositories: [],
  });
  start();
  await screen.findByText(
    "No worktrees found. Enter a repository path and scan again.",
  );
  expect(screen.getByRole("button", { name: "Scan worktrees" })).toBeEnabled();
});

it("rescans projects that arrive during the initial read, without overlapping reads", async () => {
  let finish!: (value: WorktreeInventory) => void;
  api.inspect
    .mockImplementationOnce(
      () =>
        new Promise<WorktreeInventory>((r) => {
          finish = r;
        }),
    )
    .mockResolvedValue(fixture());
  const view = render(
    <LocaleProvider>
      <WorktreesPage projects={[]} />
    </LocaleProvider>,
  );
  view.rerender(
    <LocaleProvider>
      <WorktreesPage projects={["/synthetic/late"]} />
    </LocaleProvider>,
  );
  expect(api.inspect).toHaveBeenCalledOnce();
  finish(fixture());
  await waitFor(() => expect(api.inspect).toHaveBeenCalledTimes(2));
  expect(api.inspect).toHaveBeenLastCalledWith({
    repository: null,
    target: null,
    projects: ["/synthetic/late"],
  });
});
it("does not replace an explicit repository when conversation candidates change", async () => {
  const view = start();
  await screen.findByText("feature/merged");
  fireEvent.change(screen.getByLabelText("Repository path"), {
    target: { value: "/synthetic/explicit" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Scan worktrees" }));
  await waitFor(() => expect(api.inspect).toHaveBeenCalledTimes(2));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Scan worktrees" }),
    ).toBeEnabled(),
  );
  view.rerender(
    <LocaleProvider>
      <WorktreesPage projects={["/synthetic/late"]} />
    </LocaleProvider>,
  );
  expect(api.inspect).toHaveBeenCalledTimes(2);
});
it("displays unknown for timestamps outside the supported date range", async () => {
  const data = fixture();
  data.repositories[0].worktrees[0].sourceModifiedAt = 1e20;
  api.inspect.mockResolvedValue(data);
  start();
  await screen.findByText("feature/merged");
  expect(
    within(screen.getByText("feature/merged").closest("tr")!).getByText(
      "Unknown",
    ),
  ).toBeVisible();
});
