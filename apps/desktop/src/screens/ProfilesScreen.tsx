import { useState } from "react";
import { useConfirm } from "../components/ConfirmDialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

import { createProfile, launchProfile, type ProfileInput, type ProfileView } from "../lib/api";
import { useInstallations } from "../hooks/useHarnesses";
import { useRoutes } from "../hooks/useModels";

export default function ProfilesScreen() {
  const { confirm, confirmDialog } = useConfirm();
  const qc = useQueryClient();
  const [launchNote, setLaunchNote] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [harnessType, setHarnessType] = useState("");
  const [modelRouteId, setModelRouteId] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const { data: installations } = useInstallations();
  const { data: routes } = useRoutes();
  const { data: profiles, isLoading } = useQuery({
    queryKey: ["profiles"],
    queryFn: () => invoke<ProfileView[]>("list_profiles_cmd"),
  });
  const del = useMutation({
    mutationFn: (id: string) => invoke<void>("delete_profile_cmd", { id }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["profiles"] }),
  });
  const create = useMutation({
    mutationFn: (input: ProfileInput) => createProfile(input),
    onSuccess: () => {
      setName("");
      setModelRouteId("");
      setCreateError(null);
      setShowCreate(false);
      void qc.invalidateQueries({ queryKey: ["profiles"] });
    },
    onError: (e) => setCreateError(e instanceof Error ? e.message : String(e)),
  });

  return (
    <div>
      <h1 className="text-2xl font-bold">Presets</h1>
      <p className="mt-1 text-sm text-slate-400">Launch bundles: a model choice plus environment for one harness. Launch them from a harness page or from here.</p>
      <button
        type="button"
        onClick={() => setShowCreate((v) => !v)}
        className="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500"
      >
        {showCreate ? "Cancel" : "Create preset"}
      </button>
      {showCreate && (
        <form
          className="mt-3 rounded border border-slate-700 bg-slate-800 p-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || !harnessType) return;
            create.mutate({
              name: name.trim(),
              harnessType,
              modelRouteId: modelRouteId || null,
              providerEndpointId: null,
              env: {},
              roleMappings: [],
            });
          }}
        >
          <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
            <input
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="preset name"
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm"
            />
            <select
              required
              value={harnessType}
              onChange={(e) => setHarnessType(e.target.value)}
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm"
            >
              <option value="">Choose harness…</option>
              {[...new Set((installations ?? []).map((i) => i.harness_type))].map((type) => (
                <option key={type} value={type}>{type}</option>
              ))}
            </select>
            <select
              value={modelRouteId}
              onChange={(e) => setModelRouteId(e.target.value)}
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm"
            >
              <option value="">No default model</option>
              {(routes ?? []).filter((route) => route.enabled).map((route) => (
                <option key={route.id} value={route.id}>
                  {route.display_name} · {route.provider_name}
                </option>
              ))}
            </select>
          </div>
          <p className="mt-2 text-xs text-slate-500">
            Environment variables and role mappings can be added after the preset is created.
          </p>
          <button
            type="submit"
            disabled={create.isPending || !name.trim() || !harnessType}
            className="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
          >
            {create.isPending ? "Creating…" : "Create preset"}
          </button>
          {createError && <p className="mt-2 text-xs text-red-400">Create failed: {createError}</p>}
        </form>
      )}
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
                  confirm("Delete preset?", "This cannot be undone.", () => del.mutateAsync(p.id).then(() => undefined), "Delete")
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
