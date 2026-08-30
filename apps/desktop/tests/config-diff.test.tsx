import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ConfigDiffViewer, buildLineDiff } from "../src/components/ConfigDiffViewer";

describe("configuration diff viewer", () => {
  it("keeps line numbers and groups adjacent additions/removals into changes", () => {
    const lines = buildLineDiff("one\ntwo\nthree", "one\nchanged\nthree\nfour");

    expect(lines).toEqual([
      { kind: "context", text: "one", oldLine: 1, newLine: 1 },
      { kind: "removed", text: "two", oldLine: 2, changeId: 0 },
      { kind: "added", text: "changed", newLine: 2, changeId: 0 },
      { kind: "context", text: "three", oldLine: 3, newLine: 3 },
      { kind: "added", text: "four", newLine: 4, changeId: 1 },
    ]);
  });

  it("navigates between highlighted changes and exposes the active change", () => {
    render(<ConfigDiffViewer before={"a\nb\nc"} after={"a\nB\nc\nd"} />);

    expect(screen.getByText("2 changes")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous change" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Next change" })).not.toBeDisabled();
    expect(screen.getByText("B").closest("div")).toHaveAttribute("data-active", "true");

    fireEvent.click(screen.getByRole("button", { name: "Next change" }));
    expect(screen.getByText("d").closest("div")).toHaveAttribute("data-active", "true");
    expect(screen.getByRole("button", { name: "Next change" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Previous change" }));
    expect(screen.getByText("B").closest("div")).toHaveAttribute("data-active", "true");
  });
});
