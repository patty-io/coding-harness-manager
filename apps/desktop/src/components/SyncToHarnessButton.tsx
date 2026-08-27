import { useState } from "react";
import { useInstallations } from "../hooks/useHarnesses";
import { SyncDialog } from "./SyncDialog";

/**
 * "Sync to harness…" action for library rows. Opens a harness picker, then
 * the sync dialog for the chosen harness — the sync engine carries the whole
 * registry diff for that harness, including the row you invoked it from.
 */
export function SyncToHarnessButton({ label }: { label?: string }) {
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
        className="rounded border border-slate-600 px-2 py-0.5 text-xs text-slate-300 hover:bg-slate-700"
      >
        {label ?? "Sync to harness…"}
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
              Opens the change preview for that harness. Nothing is written
              until you press Apply.
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
          onClose={() => setSyncing(null)}
        />
      )}
    </>
  );
}