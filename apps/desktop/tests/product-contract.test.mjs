import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
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
const mcp = read("src/screens/McpScreen.tsx");
const app = read("src/App.tsx");
const sidebar = read("src/components/Sidebar.tsx");
const html = read("index.html");
const theme = read("src/index.css");
const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));

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
  it("implements Patty's public inverse design tokens and interaction rules", () => {
    assert.ok(theme.includes("--patty-blue-60: #1769e0"), "theme must use Patty action cobalt");
    assert.ok(theme.includes("--patty-black: #161616"), "theme must use Patty assurance ink");
    assert.ok(theme.includes("SUIT-Variable.woff2"), "theme must bundle the Patty typeface");
    assert.ok(theme.includes("--radius-md: 4px"), "theme must retain Patty's sharp radius discipline");
    assert.ok(theme.includes(":focus-visible"), "theme must provide a global focus treatment");
    assert.ok(
      theme.includes("@media (prefers-reduced-motion: reduce)"),
      "theme must honor reduced-motion preferences",
    );
    assert.ok(theme.includes("scrollbar-color"), "theme must style native scrollbars consistently");
    assert.ok(!html.includes("#0f172a"), "cold-start shell must not flash the legacy slate background");
    assert.ok(html.includes("#161616"), "cold-start shell must use Patty assurance ink");
    assert.ok(theme.includes(".rounded {"), "default cards and controls must use Patty's square geometry");
    assert.ok(sidebar.includes("border-l-2"), "active navigation must use an evidence-style action rule");
    assert.ok(
      sidebar.includes('whitespace-nowrap text-[9px]'),
      "compact brand tagline must remain on one line",
    );
    assert.ok(
      existsSync(resolve(root, "public/fonts/SUIT-Variable.woff2")),
      "public app must contain the vendored SUIT font",
    );
    assert.ok(
      existsSync(resolve(root, "public/fonts/SUIT-OFL-1.1.txt")),
      "vendored SUIT font must include its redistribution license",
    );
  });

  it("uses the product symbol and lockup across desktop branding surfaces", () => {
    assert.ok(
      sidebar.includes('resources/logos/symbol-ui.svg'),
      "sidebar must render the compact product symbol",
    );
    assert.ok(sidebar.includes('alt=""'), "decorative sidebar symbol must have empty alt text");
    assert.ok(
      html.includes('rel="icon"') && html.includes("chm-symbol.svg"),
      "browser shell must use the product symbol as its favicon",
    );
    assert.ok(
      tauriConfig.bundle.icon.length > 0,
      "desktop bundles must declare generated platform icons",
    );
  });

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
    assert.ok(mcp.includes("groupDetectedMcps"), "MCP detections must be grouped by logical server");
    assert.ok(mcp.includes("Show configuration details"), "grouped MCPs must retain configuration details");
    assert.ok(mcp.includes("found in {group.foundIn.join(\", \")}"), "grouped MCPs must show all harnesses");
    assert.ok(app.includes("min-h-0 overflow-hidden"), "app shell must contain document scrolling");
    assert.ok(app.includes("overflow-x-hidden overflow-y-auto"), "main content must own the vertical scroll");
    assert.ok(!sourceFiles.some((file) => read(file).includes("window.confirm")), "native confirm must not be used in the webview");
  });
});
