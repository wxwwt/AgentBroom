import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { testInventory } from "../test-fixtures/inventory";
const bridge = vi.hoisted(() => ({ load: vi.fn() }));
vi.mock("./bridge", () => ({
  sourceLabel: "Native Codex scanner",
  loadInventory: bridge.load,
  errorText: (e: Error) => e.message,
}));
import App from "./App";
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
beforeEach(() => bridge.load.mockResolvedValue(testInventory()));
describe("live-only application", () => {
  it("loads real inventory automatically without demo or destructive controls", async () => {
    render(<App />);
    await screen.findByText("Where the bytes live");
    expect(bridge.load).toHaveBeenCalledWith();
    expect(screen.getByText("Local Codex · read-only")).toBeVisible();
    for (const name of [
      /demo/i,
      /sample report/i,
      /review selection/i,
      /delete/i,
    ])
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      screen.getByText("Partial inventory · measured home files"),
    ).toBeVisible();
  });
  it("retries a failed source without inventing a fallback inventory", async () => {
    bridge.load.mockRejectedValueOnce(new Error("Source inaccessible"));
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Source inaccessible",
    );
    expect(screen.queryByText("Where the bytes live")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry inspection" }));
    await screen.findByText("Where the bytes live");
    expect(bridge.load).toHaveBeenCalledTimes(2);
  });
  it("keeps only an in-memory marker without weakening protection", async () => {
    render(<App />);
    await screen.findByText("Where the bytes live");
    fireEvent.click(screen.getByRole("button", { name: /Conversations/ }));
    fireEvent.click(screen.getByRole("button", { name: /Local synthetic 0/ }));
    fireEvent.click(
      screen.getByRole("button", { name: "Keep this conversation" }),
    );
    expect(screen.getAllByText("Protected · kept this session")).toHaveLength(
      2,
    );
    expect(Object.keys(localStorage)).toHaveLength(0);
  });
});

it("keeps real inventory as the source of cleanup preview and revokes it on rescan", async () => {
  bridge.load.mockResolvedValue(testInventory());
  render(<App />);
  await screen.findByText("Where the bytes live");
  fireEvent.click(screen.getByRole("button", { name: "Cleanup preview" }));
  expect(
    screen.queryByRole("checkbox", { checked: true }),
  ).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Select Local synthetic 0" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Preview impact" }));
  expect(screen.getByText("Impact preview")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Rescan Codex" }));
  await screen.findByText("Cleanup candidates");
  expect(screen.queryByText("Impact preview")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("checkbox", { checked: true }),
  ).not.toBeInTheDocument();
});
