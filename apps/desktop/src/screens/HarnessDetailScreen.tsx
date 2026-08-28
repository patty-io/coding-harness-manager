import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  adoptHarnessModel,
  applyHarnessModelEdits,
  harnessModelsView,
  launchProfile,
  listEndpointOptions,
  listRoutes,
  readHarnessRawConfig,
  readHarnessState,
  type HarnessModelRow,
} from "../lib/api";
import {
  useHarnessDrift,
  useInstallations,
  useRecordManualSnapshot,
} from "../hooks/useHarnesses";
import { SyncDialog } from "../components/SyncDialog";
import { useMutation, useQueryClient } from "@tanstack/react-query";

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

async function listProfilesForHarness(): Promise<
  { id: string; name: string; harnessType: string; modelDisplay: string | null }[]
> {
  const { listProfiles } = await import("../lib/api");
  const all = await listProfiles();
  return all.map((p) => ({
    id: p.id,
    name: p.name,
    harnessType: p.harnessType,
    modelDisplay: p.modelDisplay,
  }));
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
  const [showDiff, setShowDiff] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [presetMenuOpen, setPresetMenuOpen] = useState(false);
  const [launchNote, setLaunchNote] = useState<string | null>(null);
  const qc = useQueryClient();

  const { data: installations } = useInstallations();
  const installation = (installations ?? []).find((i) => i.id === id);
  const { data: drift } = useHarnessDrift(id);
  const rebaseline = useRecordManualSnapshot();

  const { data: profiles } = useQuery({
    queryKey: ["profiles"],
    queryFn: listProfilesForHarness,
    enabled: !!installation?.harness_type,
  });

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

  const { data: modelRows } = useQuery({
    queryKey: ["harness-models", id],
    queryFn: () => harnessModelsView(id!),
    enabled: !!id && tab === "models",
  });

  const { data: routes } = useQuery({
    queryKey: ["routes"],
    queryFn: listRoutes,
    enabled: tab === "models",
  });

  // Library models not present on this harness (for "Add from library").
  const missingFromLibrary = (() => {
    const onHarness = new Set(
      (modelRows ?? []).map((r) => r.remoteModelId.toLowerCase()),
    );
    return (routes ?? []).filter(
      (r) => !onHarness.has(r.remote_model_id.toLowerCase()),
    );
  })();

  const [addMenuOpen, setAddMenuOpen] = useState(false);
  const [adopting, setAdopting] = useState<HarnessModelRow | null>(null);
  const [editing, setEditing] = useState<HarnessModelRow | null>(null);
  const [editName, setEditName] = useState("");
  const [editRemote, setEditRemote] = useState("");
  const [editContext, setEditContext] = useState<string>("");
  const [busyRow, setBusyRow] = useState<string | null>(null);
  const [rowNote, setRowNote] = useState<string | null>(null);
  const [adoptEndpoint, setAdoptEndpoint] = useState("");
  const { data: endpointOptions } = useQuery({
    queryKey: ["endpoint-options"],
    queryFn: listEndpointOptions,
    enabled: adopting !== null,
  });
  const adopt = useMutation({
    mutationFn: (vars: { nativeId: string; endpointId: string }) =>
      adoptHarnessModel(id!, vars.nativeId, vars.endpointId),
    onSuccess: () => {
      setAdopting(null);
      void qc.invalidateQueries({ queryKey: ["harness-models", id] });
      void qc.invalidateQueries({ queryKey: ["routes"] });
      void qc.invalidateQueries({ queryKey: ["dashboard"] });
    },
  });

  const invalidateAfterEdit = () => {
    void qc.invalidateQueries({ queryKey: ["harness-models", id] });
    void qc.invalidateQueries({ queryKey: ["harness-state", id] });
    void qc.invalidateQueries({ queryKey: ["drift", id] });
    void qc.invalidateQueries({ queryKey: ["dashboard"] });
  };

  const runOp = async (ops: {
    op: "update" | "remove" | "duplicate";
    nativeId: string;
    displayName?: string;
    contextWindow?: number;
    remoteModelId?: string;
  }[]) => {
    setRowNote(null);
    try {
      const r = await applyHarnessModelEdits(id!, ops);
      setRowNote(
        `Applied: ${r.added} added, ${r.updated} updated, ${r.removed} removed.`,
      );
      invalidateAfterEdit();
    } catch (e) {
      setRowNote(`Failed: ${String(e)}`);
    } finally {
      setBusyRow(null);
    }
  };

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

      <div className="mt-2 flex flex-wrap items-center gap-3">
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
        {drift?.drifted && (
          <span className="rounded border border-amber-500/40 bg-amber-500/10 px-2 py-0.5 text-xs text-amber-300">
            changed outside the app
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <div className="relative">
            <button
              onClick={() => setPresetMenuOpen((v) => !v)}
              className="rounded border border-slate-600 px-3 py-1 text-sm text-slate-200 hover:bg-slate-800"
            >
              Launch ▾
            </button>
            {presetMenuOpen && (
              <div className="absolute right-0 z-10 mt-1 w-64 rounded border border-slate-700 bg-slate-800 py-1 shadow-lg">
                {(() => {
                  const mine = (profiles ?? []).filter(
                    (p) => p.harnessType === installation?.harness_type,
                  );
                  if (mine.length === 0) {
                    return (
                      <p className="px-3 py-2 text-xs text-slate-500">
                        No presets for this harness yet.{" "}
                        <Link to="/profiles" className="text-blue-400 hover:underline">
                          Create one →
                        </Link>
                      </p>
                    );
                  }
                  return mine.map((p) => (
                    <button
                      key={p.id}
                      onClick={async () => {
                        setPresetMenuOpen(false);
                        setLaunchNote(null);
                        try {
                          const r = await launchProfile(p.id);
                          setLaunchNote(`Launched (pid ${r.pid ?? "?"})`);
                        } catch (e) {
                          setLaunchNote(`Launch failed: ${String(e)}`);
                        }
                        void qc;
                      }}
                      className="block w-full px-3 py-1.5 text-left text-sm text-slate-200 hover:bg-slate-700"
                    >
                      {p.name}
                      {p.modelDisplay && (
                        <span className="ml-2 text-xs text-slate-500">
                          {p.modelDisplay}
                        </span>
                      )}
                    </button>
                  ));
                })()}
              </div>
            )}
          </div>
          <button
            onClick={() => setSyncing(true)}
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500"
          >
            Review changes
          </button>
        </div>
      </div>

      {launchNote && <p className="mt-1 text-xs text-slate-400">{launchNote}</p>}

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
            {drift?.drifted && (
              <div className="mb-4 rounded border border-amber-500/40 bg-amber-500/10 p-3">
                <div className="flex items-center justify-between">
                  <div className="text-sm font-medium text-amber-300">
                    This config changed outside the app since the last apply.
                  </div>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setShowDiff((v) => !v)}
                      className="rounded border border-amber-500/50 px-2 py-0.5 text-xs text-amber-200 hover:bg-amber-500/15"
                    >
                      {showDiff ? "Hide diff" : "Show diff"}
                    </button>
                    <button
                      onClick={() => id && rebaseline.mutate(id)}
                      disabled={rebaseline.isPending}
                      className="rounded border border-amber-500/50 px-2 py-0.5 text-xs text-amber-200 hover:bg-amber-500/15 disabled:opacity-50"
                    >
                      {rebaseline.isPending ? "Saving…" : "Keep my changes"}
                    </button>
                  </div>
                </div>
                <p className="mt-1 text-xs text-amber-200/70">
                  "Keep my changes" records the file as it is now as the new
                  baseline. "Review changes" instead compares the registry with
                  disk and lets you apply.
                </p>
                {showDiff && (
                  <div className="mt-3 grid grid-cols-1 gap-3 lg:grid-cols-2">
                    <div>
                      <div className="mb-1 text-xs font-medium text-slate-400">
                        Last written by the app
                      </div>
                      <pre className="max-h-64 overflow-auto rounded border border-slate-700 bg-slate-950 p-2 font-mono text-[11px] text-slate-400">
                        {drift.lastWrittenContent ?? "(none)"}
                      </pre>
                    </div>
                    <div>
                      <div className="mb-1 text-xs font-medium text-amber-300">
                        On disk now
                      </div>
                      <pre className="max-h-64 overflow-auto rounded border border-amber-500/30 bg-slate-950 p-2 font-mono text-[11px] text-slate-200">
                        {drift.currentContent ?? "(file missing)"}
                      </pre>
                    </div>
                  </div>
                )}
              </div>
            )}
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
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-xs text-slate-500">
                Models configured on this harness, as they are on disk right
                now. Rows matching your library are marked; the rest can be
                pulled in.
              </p>
              {(() => {
                const canAdd = missingFromLibrary.length > 0;
                return (
                  <div className="relative">
                    <button
                      onClick={() => setAddMenuOpen(!addMenuOpen)}
                      disabled={!canAdd}
                      title={
                        canAdd
                          ? undefined
                          : "Everything in your library is already on this harness"
                      }
                      className="rounded border border-blue-500 px-3 py-1 text-sm text-blue-300 hover:bg-blue-500/10 disabled:opacity-40"
                    >
                      + Add from library
                    </button>
                    {addMenuOpen && canAdd && (
                      <div className="absolute right-0 z-10 mt-1 max-h-72 w-72 overflow-auto rounded border border-slate-700 bg-slate-800 py-1 shadow-lg">
                        {missingFromLibrary.map((r) => (
                          <button
                            key={r.id}
                            onClick={() => {
                              setAddMenuOpen(false);
                              setSyncing(true);
                            }}
                            className="block w-full px-3 py-1.5 text-left text-sm hover:bg-slate-700"
                          >
                            <span className="text-slate-100">{r.display_name}</span>
                            <span className="ml-2 font-mono text-xs text-slate-500">
                              {r.remote_model_id}
                            </span>
                          </button>
                        ))}
                        <p className="px-3 py-2 text-xs text-slate-500">
                          Opens the change preview — nothing is written until
                          you press Apply.
                        </p>
                      </div>
                    )}
                  </div>
                );
              })()}
            </div>

            {rowNote && (
              <p
                className={`mt-2 text-xs ${rowNote.startsWith("Failed") ? "text-red-400" : "text-green-400"}`}
              >
                {rowNote}
              </p>
            )}

            {(modelRows ?? []).length === 0 ? (
              <EmptyState>
                No models configured in this harness yet.
              </EmptyState>
            ) : (
              <table className="mt-3 w-full bg-slate-800 text-sm">
                <thead>
                  <tr className="border-b border-slate-700 text-left text-xs uppercase tracking-wide text-slate-400">
                    <th className="p-2">Native id</th>
                    <th className="p-2">Remote model</th>
                    <th className="p-2">Display name</th>
                    <th className="p-2">Provider</th>
                    <th className="p-2 text-right">Context</th>
                    <th className="p-2">Library</th>
                    <th className="p-2 text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {(modelRows ?? []).map((m) => (
                    <tr key={m.nativeId} className="border-b border-slate-700/60">
                      <td className="p-2 font-mono text-xs text-slate-300">{m.nativeId}</td>
                      <td className="p-2 font-mono text-xs text-slate-100">{m.remoteModelId}</td>
                      <td className="p-2 text-slate-300">{m.displayName}</td>
                      <td className="p-2 text-xs text-slate-400">
                        {m.providerName ? (
                          <span title={`matched via ${m.providerMatch}`}>
                            {m.providerName}
                            {m.providerMatch === "catalog" && (
                              <span className="ml-1 text-slate-600">(catalog)</span>
                            )}
                          </span>
                        ) : (
                          "—"
                        )}
                      </td>
                      <td className="p-2 text-right text-slate-400">
                        {m.contextWindow ? m.contextWindow.toLocaleString() : "—"}
                      </td>
                      <td className="p-2">
                        {m.inLibrary ? (
                          <span
                            className="rounded bg-green-500/15 px-2 py-0.5 text-xs text-green-400"
                            title={m.libraryDisplayName ?? undefined}
                          >
                            in library
                          </span>
                        ) : (
                          <button
                            onClick={() => {
                              setAdopting(m);
                              setAdoptEndpoint("");
                            }}
                            className="rounded border border-blue-500 px-2 py-0.5 text-xs text-blue-300 hover:bg-blue-500/10"
                          >
                            Save to library
                          </button>
                        )}
                      </td>
                      <td className="p-2">
                        <div className="flex items-center justify-end gap-1">
                          <button
                            onClick={() => {
                              setEditing(m);
                              setEditName(m.displayName);
                              setEditRemote(m.remoteModelId);
                              setEditContext(
                                m.contextWindow ? String(m.contextWindow) : "",
                              );
                            }}
                            className="rounded border border-slate-600 px-1.5 py-0.5 text-xs text-slate-300 hover:bg-slate-700"
                          >
                            Edit
                          </button>
                          <button
                            disabled={busyRow === m.nativeId}
                            onClick={() => {
                              setBusyRow(m.nativeId);
                              void runOp([
                                { op: "duplicate", nativeId: m.nativeId },
                              ]);
                            }}
                            className="rounded border border-slate-600 px-1.5 py-0.5 text-xs text-slate-300 hover:bg-slate-700 disabled:opacity-50"
                          >
                            Duplicate
                          </button>
                          <button
                            disabled={busyRow === m.nativeId}
                            onClick={() => {
                              if (
                                window.confirm(
                                  `Remove ${m.nativeId} from this harness's config? A backup is kept and this is undoable from History.`,
                                )
                              ) {
                                setBusyRow(m.nativeId);
                                void runOp([
                                  { op: "remove", nativeId: m.nativeId },
                                ]);
                              }
                            }}
                            className="rounded border border-red-500/50 px-1.5 py-0.5 text-xs text-red-400 hover:bg-red-500/10 disabled:opacity-50"
                          >
                            Delete
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}

            {editing && (
              <div
                className="fixed inset-0 z-20 flex items-center justify-center bg-black/60"
                onClick={() => setEditing(null)}
              >
                <div
                  className="w-[26rem] rounded border border-slate-700 bg-slate-800 p-4"
                  onClick={(e) => e.stopPropagation()}
                >
                  <h3 className="font-medium text-slate-100">
                    Edit model on this harness
                  </h3>
                  <p className="mt-1 font-mono text-xs text-slate-500">
                    native id: {editing.nativeId}
                  </p>
                  <label className="mt-3 block text-xs text-slate-500">
                    Display name
                  </label>
                  <input
                    value={editName}
                    onChange={(e) => setEditName(e.target.value)}
                    className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm text-slate-200"
                  />
                  <label className="mt-3 block text-xs text-slate-500">
                    Remote model id (renaming removes the old entry and adds
                    this one)
                  </label>
                  <input
                    value={editRemote}
                    onChange={(e) => setEditRemote(e.target.value)}
                    className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 font-mono text-xs text-slate-200"
                  />
                  <label className="mt-3 block text-xs text-slate-500">
                    Context window (tokens)
                  </label>
                  <input
                    value={editContext}
                    onChange={(e) => setEditContext(e.target.value)}
                    inputMode="numeric"
                    className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm text-slate-200"
                  />
                  <div className="mt-4 flex justify-end gap-2">
                    <button
                      onClick={() => setEditing(null)}
                      className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={() => {
                        const ctx = parseInt(editContext, 10);
                        setEditing(null);
                        void runOp([
                          {
                            op: "update",
                            nativeId: editing.nativeId,
                            displayName: editName,
                            remoteModelId: editRemote,
                            ...(Number.isFinite(ctx) ? { contextWindow: ctx } : {}),
                          },
                        ]);
                      }}
                      className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500"
                    >
                      Save changes
                    </button>
                  </div>
                </div>
              </div>
            )}

            {adopting && (
              <div
                className="fixed inset-0 z-20 flex items-center justify-center bg-black/60"
                onClick={() => setAdopting(null)}
              >
                <div
                  className="w-[28rem] rounded border border-slate-700 bg-slate-800 p-4"
                  onClick={(e) => e.stopPropagation()}
                >
                  <h3 className="font-medium text-slate-100">Save to library</h3>
                  <p className="mt-1 text-sm text-slate-400">
                    <span className="font-mono text-xs">{adopting.remoteModelId}</span>
                    {adopting.displayName !== adopting.remoteModelId && (
                      <> — {adopting.displayName}</>
                    )}
                    {adopting.contextWindow
                      ? ` · ${adopting.contextWindow.toLocaleString()} tokens`
                      : ""}
                  </p>
                  <label className="mt-3 block text-xs text-slate-500">
                    Which provider endpoint serves this model?
                  </label>
                  <select
                    value={adoptEndpoint}
                    onChange={(e) => setAdoptEndpoint(e.target.value)}
                    className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm text-slate-200"
                  >
                    <option value="">Choose an endpoint…</option>
                    {(endpointOptions ?? []).map((o) => (
                      <option key={o.endpointId} value={o.endpointId}>
                        {o.providerName} — {o.endpointName} ({o.protocol})
                      </option>
                    ))}
                  </select>
                  <div className="mt-4 flex justify-end gap-2">
                    <button
                      onClick={() => setAdopting(null)}
                      className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={() =>
                        adoptEndpoint &&
                        adopt.mutate({
                          nativeId: adopting.nativeId,
                          endpointId: adoptEndpoint,
                        })
                      }
                      disabled={!adoptEndpoint || adopt.isPending}
                      className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500 disabled:opacity-50"
                    >
                      {adopt.isPending ? "Saving…" : "Save to library"}
                    </button>
                  </div>
                  {adopt.isError && (
                    <p className="mt-2 text-xs text-red-400">
                      {adopt.error.message}
                    </p>
                  )}
                </div>
              </div>
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

      {syncing && installation && (
        <SyncDialog
          installationId={installation.id}
          harnessType={installation.harness_type}
          onClose={() => setSyncing(false)}
        />
      )}
    </div>
  );
}