import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";
import { SyncDialog } from "../components/SyncDialog";

const STATUS_STYLES: Record<string, string> = {
  installed: "bg-green-500/15 text-green-400 border border-green-500/30",
  detected: "bg-slate-700/50 text-slate-300 border border-slate-600",
  "config-missing": "bg-amber-500/15 text-amber-400 border border-amber-500/30",
  error: "bg-red-500/15 text-red-400 border border-red-500/30",
};

const STATUS_LABELS: Record<string, string> = {
  installed: "Installed",
  detected: "Detected",
  "config-missing": "Config found, no binary",
  error: "Error",
};

const HARNESS_ICONS: Record<string, string> = {
  "claude-code": "◆",
  codex: "▲",
  opencode: "●",
  pi: "■",
  reasonix: "✦",
};

export default function HarnessesScreen() {
  const navigate = useNavigate();
  const { data: installations, isLoading } = useInstallations();
  const scan = useScanHarnesses();
  const [syncing, setSyncing] = useState<{ id: string; type: string } | null>(null);
  const [query, setQuery] = useState("");

  const filtered = (installations ?? []).filter((i) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      i.harness_type.toLowerCase().includes(q) ||
      i.status.toLowerCase().includes(q) ||
      (i.version ?? "").toLowerCase().includes(q) ||
      (i.config_path ?? "").toLowerCase().includes(q)
    );
  });

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Harnesses</h1>
          <p className="mt-1 text-sm text-slate-400">
            Coding agents on this machine. Click one to see its models, MCP
            servers, and skills as they are on disk right now.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search harnesses…"
            className="w-56 rounded border border-slate-600 bg-slate-800 px-3 py-2 text-sm text-slate-200 placeholder:text-slate-500"
          />
          <button
            onClick={() => scan.mutate()}
            disabled={scan.isPending}
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
          >
            {scan.isPending ? "Scanning…" : "Scan machine"}
          </button>
        </div>
      </div>
      {scan.isError && (
        <p className="mt-2 text-red-400">Scan failed: {scan.error.message}</p>
      )}

      {isLoading && <p className="mt-4">Loading…</p>}

      <div className="mt-5 grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {filtered.map((i) => (
          <div
            key={i.id}
            onClick={() => navigate(`/harnesses/${i.id}`)}
            className="cursor-pointer rounded-lg border border-slate-700 bg-slate-800 p-4 transition-colors hover:border-slate-500"
          >
            <div className="flex items-start justify-between">
              <div className="flex items-center gap-2">
                <span className="text-lg text-slate-400">
                  {HARNESS_ICONS[i.harness_type] ?? "◇"}
                </span>
                <span className="font-medium text-slate-100">
                  {i.harness_type}
                </span>
              </div>
              <span
                className={`rounded border px-2 py-0.5 text-xs ${
                  STATUS_STYLES[i.status] ?? STATUS_STYLES.detected
                }`}
              >
                {STATUS_LABELS[i.status] ?? i.status}
              </span>
            </div>
            <div className="mt-3 space-y-1 text-xs text-slate-500">
              <div>
                version{" "}
                <span className="text-slate-300">{i.version ?? "—"}</span>
              </div>
              <div className="truncate font-mono" title={i.config_path ?? ""}>
                {i.config_path ?? "no config file"}
              </div>
            </div>
            <div className="mt-3 flex items-center justify-between border-t border-slate-700/60 pt-3">
              <span className="text-xs text-blue-400">Open →</span>
              {i.status !== "detected" && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setSyncing({ id: i.id, type: i.harness_type });
                  }}
                  title="Compare registry with this harness's config and preview changes"
                  className="rounded border border-blue-500/60 bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-300 hover:bg-blue-500/25"
                >
                  Sync from library…
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      {filtered.length === 0 && (installations ?? []).length > 0 && (
        <p className="mt-6 text-sm text-slate-500">
          No harnesses match "{query}".
        </p>
      )}
      {(installations ?? []).length === 0 && !isLoading && (
        <div className="mt-6 rounded border border-slate-700 bg-slate-800/50 p-6 text-center">
          <p className="text-slate-300">No harnesses found yet.</p>
          <Link
            to="/import"
            className="mt-3 inline-block rounded bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-500"
          >
            Import existing setup
          </Link>
        </div>
      )}
      {syncing && (
        <SyncDialog
          installationId={syncing.id}
          harnessType={syncing.type}
          onClose={() => setSyncing(null)}
        />
      )}
    </div>
  );
}