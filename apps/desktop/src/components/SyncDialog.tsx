import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import {
  syncApply,
  syncPreview,
  type SyncSelection,
} from "../lib/api";

export type { SyncSelection } from "../lib/api";

export function useSyncPreview(
  installationId: string,
  mode: string | null,
  selection?: SyncSelection,
) {
  const selectionKey = JSON.stringify(selection ?? null);
  return useQuery({
    queryKey: ["sync-preview", installationId, mode, selectionKey],
    queryFn: () => syncPreview(installationId, mode!, selection),
    enabled: !!mode && mode !== "none",
    staleTime: 30_000,
  });
}

export function useSyncApply() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      installationId,
      mode,
      force,
      planHash,
      selection,
    }: {
      installationId: string;
      mode: string;
      force: boolean;
      planHash: string;
      selection?: SyncSelection;
    }) => syncApply(installationId, mode, force, planHash, selection),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["installations"] });
      qc.invalidateQueries({ queryKey: ["routes"] });
      qc.invalidateQueries({ queryKey: ["drift"] });
      qc.invalidateQueries({ queryKey: ["harness-state"] });
      qc.invalidateQueries({ queryKey: ["harness-models"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
    },
  });
}

export function SyncDialog({
  installationId,
  harnessType,
  selection,
  onClose,
}: {
  installationId: string;
  harnessType: string;
  selection?: SyncSelection;
  onClose: () => void;
}) {
  const [mode, setMode] = useState<string>("append");
  const [force, setForce] = useState(false);
  const [expandedFile, setExpandedFile] = useState<string | null>(null);
  const preview = useSyncPreview(installationId, mode, selection);
  const apply = useSyncApply();
  const selectionScoped = !!selection;

  const ACTION_COLORS: Record<string, string> = {
    add: "bg-green-100 text-green-700",
    update: "bg-amber-100 text-amber-700",
    remove: "bg-red-100 text-red-700",
    conflict: "bg-orange-100 text-orange-700",
    unsupported: "bg-slate-700 text-slate-300",
    unchanged: "bg-slate-900 text-gray-400",
    noop: "bg-slate-900 text-gray-400",
  };

  const hasBlockers = preview.data?.hasBlockers ?? false;
  const routeBlockers = preview.data?.routeBlockers ?? [];
  const hasRouteBlockers = routeBlockers.length > 0;
  const noOp = (preview.data?.writableChanges ?? 0) === 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="sync-dialog-title"
        className="flex max-h-[85vh] w-full max-w-3xl flex-col rounded bg-slate-800 shadow-xl"
      >
        <div className="flex items-center justify-between border-b p-4">
          <div>
            <h2 id="sync-dialog-title" className="font-medium">
              Sync {harnessType}
            </h2>
            <p className="text-xs text-slate-400">
              Preview of what would change in this harness's config files.
              Nothing is written until you press Apply.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close sync dialog"
            className="text-slate-400"
          >
            ✕
          </button>
        </div>
        <div className="flex-1 overflow-auto p-4">
          <div className="flex gap-4 text-sm">
            <label className="flex items-center gap-1">
              <input
                type="radio"
                checked={mode === "append"}
                onChange={() => setMode("append")}
              />
              Append/update
            </label>
            {!selectionScoped && (
              <label className="flex items-center gap-1">
                <input
                  type="radio"
                  checked={mode === "replaceManaged"}
                  onChange={() => setMode("replaceManaged")}
                />
                Replace managed
              </label>
            )}
          </div>
          {selectionScoped && (
            <p className="mt-2 text-xs text-slate-500">
              This is a selection-scoped sync, so it only appends or updates
              the chosen resources. Replace Managed is available when syncing
              the full library.
            </p>
          )}
          {preview.isLoading && <p className="mt-3 text-sm" role="status">Computing diff…</p>}
          {preview.isError && (
            <p className="mt-3 text-sm text-red-600">{preview.error.message}</p>
          )}
          {preview.data && (
            <>
              <p className="mt-3 text-sm font-medium">{preview.data.summary}</p>
              <p className="mt-1 text-xs text-slate-500">
                Reviewed plan {preview.data.planHash.slice(0, 12)} · {preview.data.writableChanges} writable file change{preview.data.writableChanges === 1 ? "" : "s"}
              </p>
              {noOp && <p className="mt-2 text-sm text-slate-400">No changes are available to apply.</p>}
              <table className="mt-2 w-full text-sm">
                <tbody>
                  {preview.data.actions.map((a, i) => (
                    <tr key={i} className="border-b">
                      <td className="p-1 text-xs text-slate-400">{a.kind}</td>
                      <td className="p-1 font-mono text-xs">{a.identity}</td>
                      <td className="p-1">
                        <span
                          className={`rounded px-1.5 py-0.5 text-xs ${ACTION_COLORS[a.action] ?? "bg-gray-100"}`}
                        >
                          {a.action}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {hasRouteBlockers && (
                <div
                  className="mt-3 rounded border border-red-500/50 bg-red-950/40 p-3"
                  role="alert"
                >
                  <h3 className="text-sm font-medium text-red-300">
                    Provider route cannot be deployed
                  </h3>
                  {routeBlockers.map((blocker) => (
                    <div key={blocker.providerId} className="mt-2 text-xs">
                      <p className="font-medium text-slate-200">
                        {blocker.providerId}
                      </p>
                      <p className="text-red-300">{blocker.reason}</p>
                      <p className="mt-1 text-slate-400">
                        Models: {blocker.modelIds.join(", ")}
                      </p>
                    </div>
                  ))}
                  <p className="mt-2 text-xs text-slate-400">
                    Fix the provider, endpoint, protocol, or credential before
                    syncing. This safety check cannot be bypassed.
                  </p>
                </div>
              )}
              {preview.data.files.length > 0 && (
                <div className="mt-3">
                  <h3 className="text-sm font-medium">Files</h3>
                  {preview.data.files.map((f) => (
                    <div key={f.path} className="mt-1">
                      <button
                        onClick={() =>
                          setExpandedFile(expandedFile === f.path ? null : f.path)
                        }
                        className="font-mono text-xs text-blue-700 hover:underline"
                      >
                        {f.path}
                      </button>
                      {expandedFile === f.path && (
                        <div className="mt-1 grid grid-cols-2 gap-2">
                          <pre className="overflow-auto rounded bg-slate-900 p-2 text-xs">
                            {f.before ?? "(new file)"}
                          </pre>
                          <pre className="overflow-auto rounded bg-green-950 p-2 text-xs">
                            {f.after}
                          </pre>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
              {hasBlockers && !hasRouteBlockers && (
                <label className="mt-3 flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={force}
                    onChange={(e) => setForce(e.target.checked)}
                  />
                  Apply despite conflicts/unsupported (advanced)
                </label>
              )}
            </>
          )}
          {apply.isError && (
            <p className="mt-2 text-sm text-red-600">{apply.error.message}</p>
          )}
          {apply.data && (
            <p className="mt-2 text-sm text-green-700">
              {apply.data.summary} — validation{" "}
              {apply.data.validation.ok ? "passed" : "FAILED"}
              {apply.data.validation.errors.length > 0 && (
                <span className="text-red-600">
                  {" "}
                  {apply.data.validation.errors.join("; ")}
                </span>
              )}
              <Link className="ml-2 underline" to="/history">View in History</Link>
            </p>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t p-4">
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-slate-600 px-3 py-1 text-sm"
          >
            Close
          </button>
          <button
            onClick={() =>
              apply.mutate({
                installationId,
                mode,
                force,
                planHash: preview.data!.planHash,
                selection,
              })
            }
            disabled={
              apply.isPending ||
              preview.isFetching ||
              !preview.data ||
              preview.isError ||
              noOp ||
              !!apply.data ||
              hasRouteBlockers ||
              (hasBlockers && !force)
            }
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
          >
            {apply.isPending ? "Applying…" : "Apply"}
          </button>
        </div>
      </div>
    </div>
  );
}
