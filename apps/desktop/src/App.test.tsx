import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { testInventory } from "../test-fixtures/inventory";
const bridge = vi.hoisted(() => ({ load: vi.fn(), mode: vi.fn(), locks: vi.fn(), setLock: vi.fn() }));
vi.mock("./nativeCleanup", () => ({readConversationLocks: bridge.locks, setConversationLock: bridge.setLock, readNativeHistory: vi.fn().mockResolvedValue([])}));
vi.mock("./bridge", () => ({
  sourceLabel: "Native Codex scanner",
  loadInventory: bridge.load,
  loadCleanupMode: bridge.mode,
  errorText: (e: Error) => e.message,
}));
import App from "./App";
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
beforeEach(() => {
  bridge.locks.mockResolvedValue([]);
  bridge.setLock.mockImplementation((id: string, locked: boolean) => Promise.resolve(locked ? [id] : []));
  bridge.load.mockResolvedValue(testInventory());
  bridge.mode.mockResolvedValue("readOnly");
});
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
  it("defaults to unlocked and persists explicit locks across rescans", async () => {
    bridge.mode.mockResolvedValue("native");
    render(<App />);
    await screen.findByText("Where the bytes live");
    expect(screen.queryByText("Protected")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Conversations/ }));
    expect(screen.getAllByText("Unlocked").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: /Local synthetic 0/ }));
    fireEvent.click(screen.getByRole("button", { name: "Lock conversation" }));
    await screen.findByRole("button", {name:"Unlock conversation"});
    expect(bridge.setLock).toHaveBeenCalledWith("synthetic-0",true);
    bridge.locks.mockResolvedValue(["synthetic-0"]);
    fireEvent.click(screen.getByRole("button", {name:"Rescan Codex"}));
    await screen.findAllByText("Locked");
    fireEvent.click(screen.getByRole("button", { name: /Local synthetic 0/ }));
    expect(screen.getByRole("button", {name:"Unlock conversation"})).toBeEnabled();
    fireEvent.click(screen.getByRole("button", {name:"Unlock conversation"}));
    await screen.findByRole("button", {name:"Lock conversation"});
    expect(bridge.setLock).toHaveBeenLastCalledWith("synthetic-0",false);
  });
  it("does not silently clear locks if persistence fails", async () => {
    bridge.mode.mockResolvedValue("native");
    bridge.locks.mockResolvedValue(["synthetic-0"]);
    bridge.setLock.mockRejectedValue(new Error("lockStateUnavailable"));
    render(<App />); await screen.findByText("Where the bytes live");
    fireEvent.click(screen.getByRole("button", {name:/Conversations/}));
    fireEvent.click(screen.getByRole("button", {name:/Local synthetic 0/}));
    fireEvent.click(screen.getByRole("button", {name:"Unlock conversation"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("Lock state could not be read or saved");
    expect(screen.getByRole("button", {name:"Unlock conversation"})).toBeEnabled();
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

it("native test chrome identifies generated state and opens the native cleanup page", async () => {
  bridge.mode.mockResolvedValue("syntheticNativeTest");
  render(<App />);
  await screen.findAllByText("Generated native test store");
  expect(
    screen.queryByText(
      "Databases are measured without opening them. Live status is unknown; every record remains protected.",
    ),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cleanup preview" }));
  expect(
    screen.getByText(
      "Select one conversation. Its native spawned subtree is included in the preview.",
    ),
  ).toBeVisible();
});

it("lock read failure preserves inventory but disables cleanup and marks state unknown",async()=>{
  bridge.mode.mockResolvedValue("native"); bridge.locks.mockRejectedValue(new Error("lockStateUnavailable"));
  render(<App/>); await screen.findByText("Where the bytes live");
  expect(screen.getAllByText("Lock state unknown").length).toBeGreaterThan(0);
  expect(screen.queryByText("Unlocked")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"Cleanup preview"}));
  expect(screen.getByRole("alert")).toHaveTextContent("Lock state is unavailable");
  expect(screen.queryByRole("button",{name:"Preview impact"})).not.toBeInTheDocument();
});
