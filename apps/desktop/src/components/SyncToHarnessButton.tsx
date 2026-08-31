import { useEffect, useState } from "react";
import { useInstallations } from "../hooks/useHarnesses";
import { SyncDialog } from "./SyncDialog";
import type { SyncSelection } from "./SyncDialog";

export type SyncResourceSelectionKey = "modelIds" | "mcpIds" | "skillIds";

export interface SyncResourceOption {
  id: string;
  label: string;
  detail?: string;
}

/** Opens a harness picker for either an explicit resource selection or the
 * complete enabled library. The selected scope is carried into preview/apply.
 */
export function SyncToHarnessButton({
  label,
  selection,
  disabled,
  resourcePicker,
}: {
  label?: string;
  selection?: SyncSelection;
  disabled?: boolean;
  resourcePicker?: {
    title: string;
    selectionKey: SyncResourceSelectionKey;
    resources: SyncResourceOption[];
  };
}) {
  const [open, setOpen] = useState(false);
  const [resourcePickerOpen, setResourcePickerOpen] = useState(false);
  const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>([]);
  const [syncing, setSyncing] = useState<{ id: string; type: string } | null>(null);
  const { data: installations } = useInstallations();
  const effectiveSelection = resourcePicker
    ? {
        ...selection,
        [resourcePicker.selectionKey]: selectedResourceIds,
      }
    : selection;

  useEffect(() => {
    if (!resourcePickerOpen && !open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (resourcePickerOpen) setResourcePickerOpen(false);
      else setOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, resourcePickerOpen]);

  const readyInstallations = (installations ?? []).filter((i) => i.status !== "detected");

  return (
    <>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          if (resourcePicker) {
            setSelectedResourceIds(resourcePicker.resources.map((resource) => resource.id));
            setResourcePickerOpen(true);
          } else {
            setOpen(true);
          }
        }}
        disabled={disabled}
        className="rounded border border-slate-600 px-2 py-0.5 text-xs text-slate-300 hover:bg-slate-700"
      >
        {label ?? (selection ? "Push selected…" : "Sync entire library…")}
      </button>

      {resourcePickerOpen && resourcePicker && (
        <div
          className="fixed inset-0 z-40 flex items-center justify-center bg-black/60"
          onClick={() => setResourcePickerOpen(false)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="sync-resource-picker-title"
            className="flex max-h-[80vh] w-full max-w-lg flex-col rounded border border-slate-700 bg-slate-800 p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id="sync-resource-picker-title" className="font-medium text-slate-100">
              {resourcePicker.title}
            </h3>
            <p className="mt-1 text-xs text-slate-400">
              All enabled {resourcePicker.selectionKey === "mcpIds" ? "MCP servers" : "library resources"} are selected by default. Choose what to include in this harness sync.
            </p>
            <div className="mt-3 flex items-center justify-between text-xs">
              <span className="text-slate-500">
                {selectedResourceIds.length} of {resourcePicker.resources.length} selected
              </span>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => setSelectedResourceIds(resourcePicker.resources.map((resource) => resource.id))}
                  className="text-blue-400 hover:underline"
                >
                  Select all
                </button>
                <button
                  type="button"
                  onClick={() => setSelectedResourceIds([])}
                  className="text-slate-400 hover:text-slate-200"
                >
                  Clear
                </button>
              </div>
            </div>
            <ul className="mt-2 min-h-0 flex-1 space-y-1 overflow-auto rounded border border-slate-700 bg-slate-900/40 p-2">
              {resourcePicker.resources.map((resource) => (
                <li key={resource.id}>
                  <label className="flex cursor-pointer items-start gap-2 rounded px-2 py-1.5 hover:bg-slate-700/60">
                    <input
                      type="checkbox"
                      checked={selectedResourceIds.includes(resource.id)}
                      onChange={() =>
                        setSelectedResourceIds((current) =>
                          current.includes(resource.id)
                            ? current.filter((id) => id !== resource.id)
                            : [...current, resource.id],
                        )
                      }
                      className="mt-0.5"
                    />
                    <span className="min-w-0">
                      <span className="block truncate text-sm text-slate-200">{resource.label}</span>
                      {resource.detail && <span className="block truncate font-mono text-xs text-slate-500">{resource.detail}</span>}
                    </span>
                  </label>
                </li>
              ))}
              {resourcePicker.resources.length === 0 && (
                <li className="px-2 py-3 text-sm text-slate-500">No enabled resources are available.</li>
              )}
            </ul>
            <div className="mt-3 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setResourcePickerOpen(false)}
                className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={selectedResourceIds.length === 0}
                onClick={() => {
                  setResourcePickerOpen(false);
                  setOpen(true);
                }}
                className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500 disabled:opacity-50"
              >
                Choose harness…
              </button>
            </div>
          </div>
        </div>
      )}

      {open && (
        <div
          role="presentation"
          className="fixed inset-0 z-20 flex items-center justify-center bg-black/60"
          onClick={() => setOpen(false)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="sync-harness-picker-title"
            className="w-96 rounded border border-slate-700 bg-slate-800 p-4"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id="sync-harness-picker-title" className="font-medium text-slate-100">Sync to which harness?</h3>
            <p className="mt-1 text-xs text-slate-400">
              {effectiveSelection
                ? "Preview only the selected library resources for that harness. Nothing is written until you press Apply."
                : "Preview the complete enabled library for that harness. Nothing is written until you press Apply."}
            </p>
            <ul className="mt-3 max-h-72 space-y-1 overflow-auto">
              {readyInstallations.map((i) => (
                  <li key={i.id}>
                    <button
                      type="button"
                      onClick={() => {
                        setOpen(false);
                        setSyncing({ id: i.id, type: i.harness_type });
                      }}
                      className="w-full rounded px-3 py-2 text-left text-sm capitalize text-slate-200 hover:bg-slate-700"
                    >
                      {i.harness_type}
                      <span className="ml-2 text-xs text-slate-500">
                        {i.status}
                      </span>
                    </button>
                  </li>
                ))}
              {readyInstallations.length === 0 && (
                <li className="px-3 py-2 text-sm text-slate-500">
                  No harnesses ready — run a scan first.
                </li>
              )}
            </ul>
            <div className="mt-3 text-right">
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {syncing && (
        <SyncDialog
          installationId={syncing.id}
          harnessType={syncing.type}
          selection={effectiveSelection}
          onClose={() => setSyncing(null)}
        />
      )}
    </>
  );
}
