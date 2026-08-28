import { useState } from "react";
import { useConfirm } from "../components/ConfirmDialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

import { launchProfile, type ProfileView } from "../lib/api";

export default function ProfilesScreen() {
  const { confirm, confirmDialog } = useConfirm();
  const qc = useQueryClient();
  const [launchNote, setLaunchNote] = useState<string | null>(null);
  const { data: profiles, isLoading } = useQuery({
    queryKey: ["profiles"],
    queryFn: () => invoke<ProfileView[]>("list_profiles_cmd"),
  });
  const del = useMutation({
    mutationFn: (id: string) => invoke<void>("delete_profile_cmd", { id }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["profiles"] }),
  });

  return (
    <div>
      <h1 className="text-2xl font-bold">Presets</h1>
      <p className="mt-1 text-sm text-slate-400">Launch bundles: a model choice plus environment for one harness. Launch them from a harness page or from here.</p>
      {isLoading && <p className="mt-4">Loading…</p>}
      <button
        onClick={() => qc.invalidateQueries({ queryKey: ["profiles"] })}
        className="mt-3 rounded border border-slate-600 px-3 py-1 text-sm"
      >
        Refresh
      </button>
      <ul className="mt-4 space-y-2">
        {(profiles ?? []).map((p) => (
          <li key={p.id} className="flex items-center justify-between rounded border border-slate-700 bg-slate-800 p-3 text-sm">
            <div>
              <span className="font-medium">{p.name}</span>{" "}
              <span className="text-slate-400">{p.harnessType}</span>
            </div>
            <div className="flex gap-2">
              <button
                onClick={async () => {
                  try {
                    const r = await launchProfile(p.id);
                    setLaunchNote(`Launched ${r.executable} (pid ${r.pid ?? "?"})`);
                  } catch (e) {
                    setLaunchNote(`Launch failed: ${e instanceof Error ? e.message : String(e)}`);
                  }
                }}
                className="rounded border border-green-300 px-2 py-0.5 text-xs text-green-700"
              >
                Launch
              </button>
              <button
                onClick={() =>
                  confirm("Delete preset?", "This cannot be undone.", () => del.mutate(p.id), "Delete")
                }
                className="rounded border border-red-200 px-2 py-0.5 text-xs text-red-600"
              >
                Delete
              </button>
            </div>
          </li>
        ))}
      </ul>
      {launchNote && <p className="mt-3 text-sm text-slate-200">{launchNote}</p>}
      {(profiles ?? []).length === 0 && !isLoading && (
        <p className="mt-3 text-sm text-slate-400">
          No profiles yet. Create one with "create profile" in the UI.
        </p>
      )}
      {confirmDialog}
    </div>
  );
}
