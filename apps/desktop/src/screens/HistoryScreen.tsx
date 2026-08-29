import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useConfirm } from "../components/ConfirmDialog";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import type { HistoryEntry } from "../lib/api";

function listHistory(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("list_history_cmd", { limit: null });
}
function rollbackTransaction(transactionId: string): Promise<{ filesRestored: string[] }> {
  return invoke("rollback_transaction_cmd", { transactionId });
}

export default function HistoryScreen() {
  const { confirm, confirmDialog } = useConfirm();
  const qc = useQueryClient();
  const { data, isLoading } = useQuery({ queryKey: ["history"], queryFn: listHistory });
  const rollback = useMutation({
    mutationFn: rollbackTransaction,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["history"] });
      qc.invalidateQueries({ queryKey: ["installations"] });
    },
  });
  const [expanded, setExpanded] = useState<string | null>(null);

  return (
    <div>
      <h1 className="text-2xl font-bold">History</h1>
      {isLoading && <p className="mt-4">Loading…</p>}
      <ul className="mt-4 space-y-2">
        {(data ?? []).map((e) => (
          <li key={e.transactionId} className="rounded border border-slate-700 bg-slate-800 p-3 text-sm">
            <div className="flex items-center justify-between">
              <div>
                <span className="font-medium">{new Date(e.startedAt).toLocaleString()}</span>{" "}
                <span className={statusClass(e.status)}>{e.status}</span>{" "}
                <span className="text-slate-400">{e.summary ?? ""}</span>
              </div>
              <div className="flex gap-2">
                <button onClick={() => setExpanded(expanded === e.transactionId ? null : e.transactionId)} className="rounded border border-slate-600 px-2 py-0.5 text-xs">
                  View Diff
                </button>
                <button
                  onClick={() =>
                  confirm(
                    "Roll back this sync?",
                    "Files changed by this sync will be restored from the snapshot.",
                    () => rollback.mutate(e.transactionId),
                    "Roll back",
                  )
                }
                  disabled={rollback.isPending || !e.canRollback}
                  title={e.rollbackReason ?? "Rollback this transaction"}
                  className="rounded border border-blue-300 px-2 py-0.5 text-xs text-blue-700 disabled:opacity-50"
                >
                  {e.canRollback ? "Rollback" : "Not reversible"}
                </button>
              </div>
            </div>
            {!e.canRollback && e.rollbackReason && (
              <p className="mt-2 text-xs text-slate-500">{e.rollbackReason}</p>
            )}
            {expanded === e.transactionId && e.snapshots.map((s) => (
              <div key={s.path} className="mt-2 grid grid-cols-2 gap-2">
                <pre className="overflow-auto rounded bg-red-950 p-2 text-xs">{s.before ?? "(none)"}</pre>
                <pre className="overflow-auto rounded bg-green-950 p-2 text-xs">{s.after ?? "(deleted)"}</pre>
              </div>
            ))}
          </li>
        ))}
      </ul>
      {confirmDialog}
    </div>
  );
}

function statusClass(s: string): string {
  return s === "succeeded" ? "text-green-700" : s === "failed" ? "text-red-600" : "text-amber-600";
}
