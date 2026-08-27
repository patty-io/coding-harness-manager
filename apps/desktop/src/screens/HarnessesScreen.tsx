import { useState } from "react";
import { Link } from "react-router-dom";
import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";
import { SyncDialog } from "../components/SyncDialog";

function StatusBadge({ status }: { status: string }) {
  const styles: Record<string, string> = {
    installed: "bg-green-500/15 text-green-400 border border-green-500/30",
    detected: "bg-slate-700/50 text-slate-300 border border-slate-600",
    "config-missing": "bg-amber-500/15 text-amber-400 border border-amber-500/30",
    error: "bg-red-500/15 text-red-400 border border-red-500/30",
  };
  const labels: Record<string, string> = {
    installed: "Installed",
    detected: "Detected",
    "config-missing": "Config found, no binary",
    error: "Error",
  };
  return (
    <span className={`rounded px-2 py-0.5 text-xs ${styles[status] ?? styles.detected}`}>
      {labels[status] ?? status}
    </span>
  );
}

export default function HarnessesScreen() {
  const { data: installations, isLoading } = useInstallations();
  const scan = useScanHarnesses();
  const [syncing, setSyncing] = useState<{ id: string; type: string } | null>(null);

  return (
    <div>
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Harnesses</h1>
          <p className="mt-1 text-sm text-slate-400">
            Installed coding agents on this machine.
          </p>
        </div>
        <button
          onClick={() => scan.mutate()}
          disabled={scan.isPending}
          className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
        >
          {scan.isPending ? "Scanning…" : "Scan machine"}
        </button>
      </div>
      {scan.isError && (
        <p className="mt-2 text-red-400">Scan failed: {scan.error.message}</p>
      )}

      <p className="mt-4 rounded border border-slate-700 bg-slate-800/50 p-3 text-xs leading-relaxed text-slate-400">
        <strong className="text-slate-300">About syncing:</strong> "Review
        changes" compares this registry (your models, MCP servers, skills)
        with the harness's native config files and shows you an exact diff.
        <strong className="text-slate-300"> Nothing is written</strong> until
        you press Apply inside that preview.
      </p>

      {isLoading && <p className="mt-4">Loading…</p>}
      <table className="mt-4 w-full bg-slate-800 text-sm">
        <thead>
          <tr className="border-b border-slate-700 text-left text-xs uppercase tracking-wide text-slate-400">
            <th className="p-2">Harness</th>
            <th className="p-2">Status</th>
            <th className="p-2">Version</th>
            <th className="p-2">Config file</th>
            <th className="p-2 text-right">Actions</th>
          </tr>
        </thead>
        <tbody>
          {(installations ?? []).map((i) => (
            <tr key={i.id} className="border-b border-slate-700/60 hover:bg-slate-700/30">
              <td className="p-2 font-medium text-slate-100">{i.harness_type}</td>
              <td className="p-2">
                <StatusBadge status={i.status} />
              </td>
              <td className="p-2 text-slate-300">{i.version ?? "—"}</td>
              <td className="p-2 font-mono text-xs text-slate-400">
                {i.config_path ?? "—"}
              </td>
              <td className="p-2 text-right">
                {i.status === "detected" ? (
                  <span className="text-xs text-slate-500 italic">
                    Support coming soon
                  </span>
                ) : (
                  <button
                    onClick={() => setSyncing({ id: i.id, type: i.harness_type })}
                    title="Compare registry with this harness's config and preview changes"
                    className="rounded border border-blue-500/60 bg-blue-500/10 px-3 py-1 text-xs font-medium text-blue-300 hover:bg-blue-500/25"
                  >
                    Review changes →
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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