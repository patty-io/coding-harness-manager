import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  readHarnessRawConfig,
  readHarnessState,
} from "../lib/api";
import { useInstallations } from "../hooks/useHarnesses";

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

type TabId = "overview" | "models" | "mcp" | "skills" | "raw";

const TABS: { id: TabId; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "models", label: "Models" },
  { id: "mcp", label: "MCP Servers" },
  { id: "skills", label: "Skills" },
  { id: "raw", label: "Raw config" },
];

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-4 py-1.5 text-sm">
      <span className="w-40 shrink-0 text-slate-500">{label}</span>
      <span className="min-w-0 break-all text-slate-200">{children}</span>
    </div>
  );
}

function EmptyState({ children }: { children: React.ReactNode }) {
  return (
    <div className="mt-4 rounded border border-dashed border-slate-700 bg-slate-800/40 p-6 text-center text-sm text-slate-500">
      {children}
    </div>
  );
}

export default function HarnessDetailScreen() {
  const { id } = useParams<{ id: string }>();
  const [tab, setTab] = useState<TabId>("overview");

  const { data: installations } = useInstallations();
  const installation = (installations ?? []).find((i) => i.id === id);

  const { data: state, isLoading: stateLoading, error: stateError } = useQuery({
    queryKey: ["harness-state", id],
    queryFn: () => readHarnessState(id!),
    enabled: !!id,
    retry: false,
  });

  const { data: rawConfig, isLoading: rawLoading } = useQuery({
    queryKey: ["harness-raw", id],
    queryFn: () => readHarnessRawConfig(id!),
    enabled: !!id && tab === "raw",
    retry: false,
  });

  if (stateLoading) {
    return <p className="text-sm text-slate-400">Reading harness config from disk…</p>;
  }

  if (stateError) {
    return (
      <div>
        <Link to="/harnesses" className="text-sm text-slate-400 hover:text-slate-200">
          ← Harnesses
        </Link>
        <p className="mt-4 rounded border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
          Could not read this harness's config: {(stateError as Error).message}
        </p>
      </div>
    );
  }

  const counts = {
    models: state?.models.length ?? 0,
    mcp: state?.mcp.length ?? 0,
    skills: state?.skills.length ?? 0,
  };

  return (
    <div>
      <Link to="/harnesses" className="text-sm text-slate-400 hover:text-slate-200">
        ← Harnesses
      </Link>

      <div className="mt-2 flex items-center gap-3">
        <h1 className="text-2xl font-bold capitalize text-slate-100">
          {installation?.harness_type ?? "Harness"}
        </h1>
        {installation && (
          <span
            className={`rounded border px-2 py-0.5 text-xs ${
              STATUS_STYLES[installation.status] ?? STATUS_STYLES.detected
            }`}
          >
            {STATUS_LABELS[installation.status] ?? installation.status}
          </span>
        )}
        {installation?.version && (
          <span className="rounded bg-slate-700 px-2 py-0.5 text-xs text-slate-300">
            v{installation.version}
          </span>
        )}
      </div>

      {installation?.config_path && (
        <p className="mt-1 font-mono text-xs text-slate-500">
          {installation.config_path}
        </p>
      )}

      <div className="mt-4 flex gap-1 border-b border-slate-700">
        {TABS.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`rounded-t px-4 py-2 text-sm ${
              tab === t.id
                ? "border-x border-t border-slate-700 bg-slate-800 font-medium text-slate-100"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            {t.label}
            {t.id === "models" && counts.models > 0 && (
              <span className="ml-1.5 rounded bg-slate-700 px-1.5 text-xs text-slate-300">
                {counts.models}
              </span>
            )}
            {t.id === "mcp" && counts.mcp > 0 && (
              <span className="ml-1.5 rounded bg-slate-700 px-1.5 text-xs text-slate-300">
                {counts.mcp}
              </span>
            )}
            {t.id === "skills" && counts.skills > 0 && (
              <span className="ml-1.5 rounded bg-slate-700 px-1.5 text-xs text-slate-300">
                {counts.skills}
              </span>
            )}
          </button>
        ))}
      </div>

      <div className="mt-4">
        {tab === "overview" && (
          <div>
            {state && state.warnings.length > 0 && (
              <div className="mb-4 rounded border border-amber-500/30 bg-amber-500/10 p-3">
                <div className="text-sm font-medium text-amber-300">Warnings</div>
                <ul className="mt-1 list-inside list-disc text-xs text-amber-200/80">
                  {state.warnings.map((w, i) => (
                    <li key={i}>{w}</li>
                  ))}
                </ul>
              </div>
            )}
            <div className="rounded border border-slate-700 bg-slate-800 p-4">
              <Row label="Models configured">{counts.models}</Row>
              <Row label="MCP servers attached">{counts.mcp}</Row>
              <Row label="Skills installed">{counts.skills}</Row>
              <Row label="Source">
                read from this machine's config files — nothing here is stored
                by us until you import
              </Row>
            </div>
            <p className="mt-3 text-xs text-slate-500">
              Editing is not wired up yet — this page currently mirrors what is
              on disk. Changing values from inside the app lands next.
            </p>
          </div>
        )}

        {tab === "models" && (
          <div>
            {(state?.models.length ?? 0) === 0 ? (
              <EmptyState>
                No models configured in this harness yet.
              </EmptyState>
            ) : (
              <table className="w-full bg-slate-800 text-sm">
                <thead>
                  <tr className="border-b border-slate-700 text-left text-xs uppercase tracking-wide text-slate-400">
                    <th className="p-2">Native id</th>
                    <th className="p-2">Remote model</th>
                    <th className="p-2">Display name</th>
                    <th className="p-2 text-right">Context</th>
                  </tr>
                </thead>
                <tbody>
                  {(state?.models ?? []).map((m) => (
                    <tr key={m.native_id} className="border-b border-slate-700/60">
                      <td className="p-2 font-mono text-xs text-slate-300">{m.native_id}</td>
                      <td className="p-2 font-mono text-xs text-slate-100">{m.remote_model_id}</td>
                      <td className="p-2 text-slate-300">{m.display_name}</td>
                      <td className="p-2 text-right text-slate-400">
                        {m.context_window ? m.context_window.toLocaleString() : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {tab === "mcp" && (
          <div>
            {(state?.mcp.length ?? 0) === 0 ? (
              <EmptyState>No MCP servers attached.</EmptyState>
            ) : (
              <table className="w-full bg-slate-800 text-sm">
                <thead>
                  <tr className="border-b border-slate-700 text-left text-xs uppercase tracking-wide text-slate-400">
                    <th className="p-2">Name</th>
                    <th className="p-2">Transport</th>
                    <th className="p-2">Command</th>
                  </tr>
                </thead>
                <tbody>
                  {(state?.mcp ?? []).map((m) => (
                    <tr key={m.native_name} className="border-b border-slate-700/60">
                      <td className="p-2 text-slate-100">{m.native_name}</td>
                      <td className="p-2 text-slate-400">{m.transport}</td>
                      <td className="p-2 font-mono text-xs text-slate-300">{m.command ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {tab === "skills" && (
          <div>
            {(state?.skills.length ?? 0) === 0 ? (
              <EmptyState>No skills installed.</EmptyState>
            ) : (
              <ul className="space-y-1.5">
                {(state?.skills ?? []).map((s) => (
                  <li
                    key={s.name}
                    className="flex items-center justify-between rounded border border-slate-700 bg-slate-800 px-3 py-2 text-sm"
                  >
                    <span className="text-slate-100">{s.name}</span>
                    <span
                      className={`rounded px-2 py-0.5 text-xs ${
                        s.symlinked
                          ? "bg-blue-500/15 text-blue-300"
                          : "bg-slate-700 text-slate-300"
                      }`}
                    >
                      {s.symlinked ? "symlinked" : "copied"}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {tab === "raw" && (
          <div>
            {rawLoading && <p className="text-sm text-slate-400">Reading config file…</p>}
            {!rawLoading && rawConfig === undefined && (
              <EmptyState>Scroll to this tab to load the raw config.</EmptyState>
            )}
            {typeof rawConfig === "string" ? (
              <pre className="max-h-[60vh] overflow-auto rounded border border-slate-700 bg-slate-950 p-4 font-mono text-xs leading-relaxed text-slate-300">
                {rawConfig}
              </pre>
            ) : null}
          </div>
        )}
      </div>
    </div>
  );
}