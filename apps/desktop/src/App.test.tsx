import { afterEach, describe, expect, it } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import App from "./App";
afterEach(cleanup);
async function start() {
  render(<App />);
  await screen.findByText("Know what’s taking up space.");
  await screen.findByText("Where the bytes live");
}
describe("fixture-only prototype", () => {
  it("shows honest source and read-only measurements", async () => {
    await start();
    expect(screen.getByText("Demo dataset · read-only")).toBeVisible();
    expect(screen.getByText("Browser fixture catalog")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /delete/i }),
    ).not.toBeInTheDocument();
  });
  it("filters and shows independent fork protection", async () => {
    await start();
    fireEvent.click(screen.getByRole("button", { name: /Conversations/ }));
    fireEvent.change(
      screen.getByRole("textbox", { name: "Search conversations" }),
      { target: { value: "Atlas dashboard" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Build the Atlas dashboard/ }),
    );
    expect(screen.getByText("Referenced by an independent fork")).toBeVisible();
    expect(
      screen.getByRole("checkbox", {
        name: "Select Build the Atlas dashboard",
      }),
    ).toBeDisabled();
  });
  it("keeps a record, removes selection, and never enables cleanup", async () => {
    await start();
    fireEvent.click(screen.getByRole("button", { name: /Conversations/ }));
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select Refactor the Orbit API" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Refactor the Orbit API/ }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Keep this conversation" }),
    );
    expect(
      screen.getByRole("checkbox", { name: "Select Refactor the Orbit API" }),
    ).not.toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: "Select Refactor the Orbit API" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Review selection" }),
    ).toBeDisabled();
  });
  it("previews only selected and owned records with no execution", async () => {
    await start();
    fireEvent.click(screen.getByRole("button", { name: /Conversations/ }));
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select Refactor the Orbit API" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Review selection" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Execution is disabled.")).toBeVisible();
    expect(within(dialog).getByText("Review API test coverage")).toBeVisible();
    expect(
      within(dialog).queryByText("Explore an alternative layout"),
    ).not.toBeInTheDocument();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Close dialog" }),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("shows a JSON report, not a backup", async () => {
    await start();
    fireEvent.click(screen.getByRole("button", { name: "View sample report" }));
    const textarea = await screen.findByRole("textbox", {
      name: "Sample JSON report",
    });
    expect(
      JSON.parse((textarea as HTMLTextAreaElement).value).executionEnabled,
    ).toBe(false);
  });
});
