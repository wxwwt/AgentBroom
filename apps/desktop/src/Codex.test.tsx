import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { testInventory } from "../test-fixtures/inventory";
const api = vi.hoisted(() => ({ load: vi.fn() }));
vi.mock("./bridge", () => ({
  loadInventory: api.load,
  loadCleanupMode: async () => "readOnly",
  errorText: (e: Error) => e.message,
}));
import App from "./App";
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
describe("conversation inspection", () => {
  it("paginates all records and follows parent/child relationships", async () => {
    api.load.mockResolvedValue(testInventory(65));
    render(<App />);
    await screen.findByText("Where the bytes live");
    fireEvent.click(screen.getByRole("button", { name: "Conversations" }));
    expect(screen.getByRole("table").querySelectorAll("tbody tr")).toHaveLength(
      50,
    );
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByRole("table").querySelectorAll("tbody tr")).toHaveLength(
      15,
    );
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect Local synthetic 1" }),
    );
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Fork lineage")).toBeVisible();
    fireEvent.click(
      within(dialog).getAllByRole("button", { name: "Local synthetic 0" })[0],
    );
    expect(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Local synthetic 2",
      }),
    ).toBeVisible();
  });
  it("does not turn missing indexed files into measured zero bytes", async () => {
    const inv = testInventory();
    inv.threads[0].files = [];
    inv.threads[0].relation = "unknown";
    api.load.mockResolvedValue(inv);
    render(<App />);
    await screen.findByText("Where the bytes live");
    fireEvent.click(screen.getByRole("button", { name: "Conversations" }));
    expect(screen.getByText("Unattributed")).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect Local synthetic 0" }),
    );
    expect(
      within(screen.getByRole("dialog")).getAllByText("Unknown / unattributed"),
    ).toHaveLength(2);
  });
});
