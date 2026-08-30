import { useState } from "react";
import { useConfirm } from "../components/ConfirmDialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  createProfile,
  deleteProfile,
  listEndpointOptions,
  launchProfile,
  listProfiles,
  updateProfile,
  type ProfileView,
  type ProfileInput,
} from "../lib/api";
import { useInstallations } from "../hooks/useHarnesses";
import { useRoutes } from "../hooks/useModels";

export default function ProfilesScreen() {
  const { confirm, confirmDialog } = useConfirm();
  const qc = useQueryClient();
  const [launchNote, setLaunchNote] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [editing, setEditing] = useState<ProfileView | null>(null);
  const [name, setName] = useState("");
  const [harnessType, setHarnessType] = useState("");
  const [modelRouteId, setModelRouteId] = useState("");
  const [providerEndpointId, setProviderEndpointId] = useState("");
  const [envText, setEnvText] = useState("{}");
  const [rolesText, setRolesText] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const { data: installations } = useInstallations();
  const { data: routes } = useRoutes();
  const { data: endpoints } = useQuery({ queryKey: ["endpoint-options"], queryFn: listEndpointOptions });
  const { data: profiles, isLoading } = useQuery({
    queryKey: ["profiles"],
    queryFn: listProfiles,
  });
  const del = useMutation({
    mutationFn: deleteProfile,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["profiles"] }),
  });
  const create = useMutation({
    mutationFn: (input: ProfileInput) => createProfile(input),
    onSuccess: () => {
      setName("");
      setModelRouteId("");
      setProviderEndpointId("");
      setEnvText("{}");
      setRolesText("");
      setCreateError(null);
      setShowCreate(false);
      void qc.invalidateQueries({ queryKey: ["profiles"] });
    },
    onError: (e) => setCreateError(e instanceof Error ? e.message : String(e)),
  });
  const update = useMutation({
    mutationFn: (input: ProfileInput) => updateProfile(editing!.id, input),
    onSuccess: () => {
      setEditing(null);
      setShowCreate(false);
      setCreateError(null);
      void qc.invalidateQueries({ queryKey: ["profiles"] });
    },
    onError: (e) => setCreateError(e instanceof Error ? e.message : String(e)),
  });

  const beginEdit = (profile: ProfileView) => {
    setEditing(profile);
    setShowCreate(true);
    setName(profile.name);
    setHarnessType(profile.harnessType);
    setModelRouteId(profile.modelRouteId ?? "");
    setProviderEndpointId(profile.providerEndpointId ?? "");
    setEnvText(JSON.stringify(profile.env, null, 2));
    setRolesText(profile.roleMappings.map((mapping) => `${mapping.role}=${mapping.model}`).join("\n"));
    setCreateError(null);
  };

  return (
    <div>
      <h1 className="text-2xl font-bold">Profiles</h1>
      <p className="mt-1 text-sm text-slate-400">Reusable launch bundles: harness, endpoint/model, environment, and role mappings.</p>
      <button
        type="button"
        onClick={() => {
          setShowCreate((v) => !v);
          if (showCreate) setEditing(null);
        }}
        className="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500"
      >
        {showCreate ? "Cancel" : "Create profile"}
      </button>
      {showCreate && (
        <form
          className="mt-3 rounded border border-slate-700 bg-slate-800 p-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || !harnessType) return;
            let env: Record<string, unknown>;
            try {
              const parsed = JSON.parse(envText);
              if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
                throw new Error("environment must be a JSON object");
              }
              env = parsed as Record<string, unknown>;
            } catch (error) {
              setCreateError(error instanceof Error ? error.message : String(error));
              return;
            }
            const roleMappings = rolesText
              .split("\n")
              .map((line) => line.trim())
              .filter(Boolean)
              .map((line) => {
                const separator = line.indexOf("=");
                return {
                  role: separator < 0 ? line : line.slice(0, separator).trim(),
                  model: separator < 0 ? "" : line.slice(separator + 1).trim(),
                };
              })
              .filter((mapping) => mapping.role && mapping.model);
            const input = {
              name: name.trim(),
              harnessType,
              modelRouteId: modelRouteId || null,
              providerEndpointId: providerEndpointId || null,
              env,
              roleMappings,
            };
            if (editing) update.mutate(input);
            else create.mutate(input);
          }}
        >
          <h2 className="font-medium text-slate-200">{editing ? "Edit profile" : "Create profile"}</h2>
          <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-3">
            <input
              required
              aria-label="Profile name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="profile name"
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm"
            />
            <select
              required
              aria-label="Harness type"
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
              aria-label="Default model"
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
            <select
              aria-label="Provider endpoint"
              value={providerEndpointId}
              onChange={(e) => setProviderEndpointId(e.target.value)}
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm"
            >
              <option value="">No endpoint override</option>
              {(endpoints ?? []).map((endpoint) => (
                <option key={endpoint.endpointId} value={endpoint.endpointId}>
                  {endpoint.providerName} · {endpoint.endpointName}
                </option>
              ))}
            </select>
          </div>
          <label className="mt-3 block text-xs text-slate-500">Environment JSON</label>
          <textarea aria-label="Environment JSON" value={envText} onChange={(e) => setEnvText(e.target.value)} rows={3} className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1 font-mono text-xs" />
          <label className="mt-3 block text-xs text-slate-500">Role mappings (one role=model per line)</label>
          <textarea aria-label="Role mappings" value={rolesText} onChange={(e) => setRolesText(e.target.value)} rows={2} className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" />
          <button
            type="submit"
            disabled={create.isPending || update.isPending || !name.trim() || !harnessType}
            className="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
          >
            {create.isPending || update.isPending ? "Saving…" : editing ? "Save profile" : "Create profile"}
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
              {p.modelDisplay && <span className="ml-2 text-xs text-slate-500">· {p.modelDisplay}</span>}
              {p.providerName && <span className="ml-2 text-xs text-slate-500">· {p.providerName}</span>}
              {(Object.keys(p.env).length > 0 || p.roleMappings.length > 0) && (
                <details className="mt-1 text-xs text-slate-500">
                  <summary className="cursor-pointer">Details</summary>
                  {Object.keys(p.env).length > 0 && <div>env: {Object.keys(p.env).join(", ")}</div>}
                  {p.roleMappings.length > 0 && <div>roles: {p.roleMappings.map((mapping) => `${mapping.role}=${mapping.model}`).join(", ")}</div>}
                </details>
              )}
            </div>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => beginEdit(p)}
                className="rounded border border-slate-600 px-2 py-0.5 text-xs text-slate-300 hover:bg-slate-700"
              >
                Edit
              </button>
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
                  confirm("Delete profile?", "This cannot be undone.", () => del.mutateAsync(p.id), "Delete")
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
