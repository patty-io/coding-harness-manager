import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "../src/components/ConfirmDialog";
import { Field } from "../src/components/Field";
import { groupDetectedMcps } from "../src/lib/mcpGrouping";

describe("shared UI contracts", () => {
  it("gives confirmation dialogs modal semantics, traps focus, and restores focus", async () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>Open</button>
          {open && (
            <ConfirmDialog
              title="Delete model"
              message="This cannot be undone."
              confirmLabel="Delete"
              onConfirm={() => undefined}
              onClose={() => setOpen(false)}
            />
          )}
        </>
      );
    }

    const user = userEvent.setup();
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Open" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Delete model" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("button", { name: "Delete" })).toHaveFocus();

    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(opener).toHaveFocus());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps a destructive dialog open while async work is pending and on failure", async () => {
    let reject!: (reason: Error) => void;
    const pending = new Promise<void>((_, fail) => {
      reject = fail;
    });
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmDialog
        title="Delete"
        message="Confirm deletion"
        onConfirm={() => pending}
        onClose={onClose}
      />,
    );
    const submit = screen.getByRole("button", { name: "Confirm" });
    await user.click(submit);
    expect(submit).toBeDisabled();
    reject(new Error("write failed"));
    expect(await screen.findByRole("alert")).toHaveTextContent("write failed");
    expect(onClose).not.toHaveBeenCalled();
    expect(submit).not.toBeDisabled();
  });

  it("associates labels, descriptions, and validation errors with controls", () => {
    render(
      <Field id="model-id" label="Model id" description="Unique on this harness." error="Already exists" required>
        <input />
      </Field>,
    );
    const input = screen.getByRole("textbox", { name: "Model id" });
    expect(input).toHaveAttribute("id", "model-id");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input.getAttribute("aria-describedby")).toContain("model-id-description");
    expect(input.getAttribute("aria-describedby")).toContain("model-id-error");
  });

  it("groups MCP detections by logical name while retaining distinct configurations", () => {
    const groups = groupDetectedMcps([
      {
        name: "lightpanda",
        transport: "http",
        command: null,
        args: [],
        url: "http://127.0.0.1:9333/mcp",
        env: {},
        foundIn: ["claude-code"],
        inLibrary: false,
      },
      {
        name: "LightPanda",
        transport: "http",
        command: null,
        args: [],
        url: "http://127.0.0.1:9336/mcp",
        env: {},
        foundIn: ["pi"],
        inLibrary: false,
      },
    ]);

    expect(groups).toHaveLength(1);
    expect(groups[0].name).toBe("lightpanda");
    expect(groups[0].foundIn).toEqual(["claude-code", "pi"]);
    expect(groups[0].entries).toHaveLength(2);
  });
});
