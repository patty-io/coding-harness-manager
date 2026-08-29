import { useState } from "react";
import { useInstallations } from "../hooks/useHarnesses";
import { SyncDialog } from "./SyncDialog";
import type { SyncSelection } from "./SyncDialog";

/** Opens a harness picker for either an explicit resource selection or the
 * complete enabled library. The selected scope is carried into preview/apply.
 */
export function SyncToHarnessButton({
  label,
  selection,
  disabled,
}: {
  label?: string;
  selection?: SyncSelection;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [syncing, setSyncing] = useState<{ id: string; type: string } | null>(null);
  const { data: installations } = useInstallations();

  return (
    <>
      <button
        onClick={(e) => {
          e.stopPropagation();
          setOpen(true);
        }}
        disabled={disabled}
        className="rounded border border-slate-600 px-2 py-0.5 text-xs text-slate-300 hover:bg-slate-700"
      >
        {label ?? (selection ? "Push selected…" : "Sync entire library…")}
      </button>

      {open && (
        <div
          className="fixed inset-0 z-20 flex items-center justify-center bg-black/60"
          onClick={() => setOpen(false)}
        >
          <div
            className="w-96 rounded border border-slate-700 bg-slate-800 p-4"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="font-medium text-slate-100">Sync to which harness?</h3>
            <p className="mt-1 text-xs text-slate-400">
              {selection
                ? "Preview only the selected library resources for that harness. Nothing is written until you press Apply."
                : "Preview the complete enabled library for that harness. Nothing is written until you press Apply."}
            </p>
            <ul className="mt-3 max-h-72 space-y-1 overflow-auto">
              {(installations ?? [])
                .filter((i) => i.status !== "detected")
                .map((i) => (
                  <li key={i.id}>
                    <button
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
              {(installations ?? []).filter((i) => i.status !== "detected")
                .length === 0 && (
                <li className="px-3 py-2 text-sm text-slate-500">
                  No harnesses ready — run a scan first.
                </li>
              )}
            </ul>
            <div className="mt-3 text-right">
              <button
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
          selection={selection}
          onClose={() => setSyncing(null)}
        />
      )}
    </>
  );
}
