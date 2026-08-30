import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

const harness = read("src/screens/HarnessDetailScreen.tsx");
const harnesses = read("src/screens/HarnessesScreen.tsx");
const harnessLogo = read("src/components/HarnessLogo.tsx");
const dashboard = read("src/screens/DashboardScreen.tsx");
const configDiff = read("src/components/ConfigDiffViewer.tsx");
const sync = read("src/components/SyncDialog.tsx");
const html = read("index.html");

const sourceFiles = [
  "src/components/ConfirmDialog.tsx",
  "src/screens/HarnessDetailScreen.tsx",
  "src/screens/ProvidersScreen.tsx",
  "src/screens/ModelsScreen.tsx",
  "src/screens/McpScreen.tsx",
  "src/screens/ProfilesScreen.tsx",
  "src/screens/HistoryScreen.tsx",
];

describe("product contracts", () => {
  it("keeps UI scope and safety invariants explicit", () => {
    assert.equal((html.match(/<body\b/g) ?? []).length, 1, "HTML must contain one body");
    assert.equal((html.match(/id=\"root\"/g) ?? []).length, 1, "HTML must contain one root");
    assert.ok(harness.includes('role="tablist"'), "harness tabs must expose a tablist");
    assert.ok(harness.includes('role="tabpanel"'), "harness content must expose a tabpanel");
    assert.ok(harness.includes("Sync from library…"), "sync action must state its direction");
    assert.ok(harness.includes("Accept local changes"), "drift action must accept local changes explicitly");
    assert.ok(harness.includes("Revert to last app baseline"), "drift action must expose an explicit revert");
    assert.ok(harness.includes('title="Save this harness model into your My Models library"'), "row action must state its direction");
    assert.ok(harness.includes("<ConfigDiffViewer"), "drift view must render a real diff");
    assert.ok(harnesses.includes("<HarnessLogo"), "harness cards must render real logos");
    for (const id of [
      "claude-code",
      "codex",
      "opencode",
      "pi",
      "reasonix",
      "kimi-cli",
      "gemini-cli",
      "qwen-code",
      "cursor",
      "cline",
      "roo-code",
      "aider",
      "amp",
      "goose",
      "continue",
    ]) {
      const key = id.includes("-") ? `"${id}"` : `${id}:`;
      assert.ok(harnessLogo.includes(key), `logo registry must include ${id}`);
    }
    assert.ok(harnessLogo.includes("maskImage"), "harness logos must remain colorable");
    assert.ok(dashboard.includes("Recent activity"), "dashboard must expose recent activity");
    assert.ok(dashboard.includes("break-words"), "activity descriptions must be readable when they wrap");
    assert.ok(!dashboard.includes('className="truncate text-slate-300"'), "activity descriptions must not be truncated");
    assert.ok(dashboard.includes("Changed outside app"), "drift state must not displace card metrics");
    assert.ok(!dashboard.includes("adapter available"), "dashboard must not expose adapter implementation status");
    assert.ok(!dashboard.includes("no config adapter"), "dashboard must not expose detection-only implementation status");
    assert.ok(!dashboard.includes("checking adapter"), "dashboard must not expose adapter loading status");
    assert.ok(configDiff.includes('aria-label="Previous change"'), "diff must support previous-change navigation");
    assert.ok(configDiff.includes('aria-label="Next change"'), "diff must support next-change navigation");
    assert.ok(sync.includes("planHash"), "sync apply must carry the validated plan hash");
    assert.ok(sync.includes("selection"), "sync apply must carry explicit selection");
    assert.ok(!sourceFiles.some((file) => read(file).includes("window.confirm")), "native confirm must not be used in the webview");
  });
});
