import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SyncDialog } from "../src/components/SyncDialog";
import { syncPreview } from "../src/lib/api";

vi.mock("../src/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../src/lib/api")>();
  return {
    ...actual,
    syncPreview: vi.fn(),
    syncApply: vi.fn(),
  };
});

describe("SyncDialog route blockers", () => {
  beforeEach(() => vi.clearAllMocks());

  it("does not offer force or Apply when a provider route is blocked", async () => {
    vi.mocked(syncPreview).mockResolvedValue({
      summary: "1 incompatible provider route",
      actions: [
        {
          kind: "provider-route",
          identity: "yolo-auto",
          action: "unsupported",
        },
      ],
      files: [],
      planHash: "abc123",
      writableChanges: 1,
      hasBlockers: true,
      routeBlockers: [
        {
          providerId: "yolo-auto",
          modelIds: ["qwen3.8-27b"],
          reason: "OpenAI Chat is not supported by this harness",
        },
      ],
    });

    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <BrowserRouter>
          <SyncDialog
            installationId="installation"
            harnessType="codex"
            onClose={() => {}}
          />
        </BrowserRouter>
      </QueryClientProvider>,
    );

    expect(
      await screen.findByText("OpenAI Chat is not supported by this harness"),
    ).toBeVisible();
    expect(screen.queryByText(/apply despite/i)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply" })).toBeDisabled();
  });
});
