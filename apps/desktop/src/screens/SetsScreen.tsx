import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useConfirm } from "../components/ConfirmDialog";
import { HelpTip } from "../components/HelpTip";
import {
  addSetItem,
  applySet,
  applySetPreview,
  createSet,
  deleteSet,
  listMcp,
  listSets,
  listSkills,
  listRoutes,
  removeSetItem,
  type SetPreviewReport,
  type SetView,
} from "../lib/api";
import { useInstallations } from "../hooks/useHarnesses";

type ItemType = "model_route" | "mcp_server" | "skill";

const ITEM_LABELS: Record<ItemType, string> = {
  model_route: "Model",
  mcp_server: "MCP server",
  skill: "Skill",
};

function SetCard({
  set,
  choices,
  installations,
}: {
  set: SetView;
  choices: Record<ItemType, { id: string; label: string }[]>;
  installations: { id: string; harness_type: string }[];
}) {
  const qc = useQueryClient();
  const { confirm, confirmDialog } = useConfirm();
  const [itemType, setItemType] = useState<ItemType>("model_route");
  const [itemId, setItemId] = useState("");
  const [installationId, setInstallationId] = useState(installations[0]?.id ?? "");
  const [mode, setMode] = useState("append");
  const [preview, setPreview] = useState<SetPreviewReport | null>(null);
  const add = useMutation({
    mutationFn: () => addSetItem(set.id, itemType, itemId),
    onSuccess: () => {
      setItemId("");
      void qc.invalidateQueries({ queryKey: ["sets"] });
    },
  });
  const remove = useMutation({
    mutationFn: (item: { itemType: string; itemId: string }) =>
      removeSetItem(set.id, item.itemType, item.itemId),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["sets"] }),
  });
  const previewMutation = useMutation({
    mutationFn: () => applySetPreview(set.id, installationId, mode),
    onSuccess: setPreview,
  });
  const apply = useMutation({
    mutationFn: () =>
      preview
        ? applySet(set.id, installationId, mode, preview.planHash)
        : Promise.reject(new Error("Preview the set before applying it")),
    onSuccess: (result) => setPreview({
      summary: result.summary,
      actions: [],
      files: result.filesWritten.map((path) => ({ path, before: null, after: null })),
      planHash: "applied",
      writableChanges: result.filesWritten.length,
      protectedChanges: 0,
      hasBlockers: !result.validation.ok,
      warnings: [],
      routeBlockers: [],
    }),
  });
  const selectedChoices = choices[itemType];

  return (
    <li className="rounded border border-slate-700 bg-slate-800 p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="font-medium text-slate-100">{set.name}</h2>
          {set.description && <p className="mt-1 text-sm text-slate-400">{set.description}</p>}
        </div>
        <button
          type="button"
          onClick={() =>
            confirm(
              `Delete ${set.name}?`,
              "The set and its membership list will be removed.",
              () => deleteSet(set.id).then(() => qc.invalidateQueries({ queryKey: ["sets"] })),
              "Delete",
            )
          }
          className="rounded border border-red-500/50 px-2 py-1 text-xs text-red-300 hover:bg-red-500/10"
        >
          Delete
        </button>
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {set.items.length === 0 && <span className="text-xs text-slate-500">No items yet.</span>}
        {set.items.map((item) => (
          <span key={`${item.itemType}:${item.itemId}`} className="inline-flex items-center gap-1 rounded bg-slate-700 px-2 py-1 text-xs text-slate-300">
            {ITEM_LABELS[item.itemType as ItemType] ?? item.itemType}: {item.itemId.slice(0, 8)}
            <button
              type="button"
              aria-label={`Remove ${item.itemType} from ${set.name}`}
              onClick={() => remove.mutate(item)}
              className="text-slate-500 hover:text-red-300"
            >
              ×
            </button>
          </span>
        ))}
      </div>
      <div className="mt-4 rounded border border-slate-700/80 bg-slate-900/40 p-3">
        <div className="text-xs font-medium uppercase tracking-wide text-slate-500">Add an item</div>
        <div className="mt-2 flex flex-wrap gap-2">
          <select value={itemType} onChange={(e) => { setItemType(e.target.value as ItemType); setItemId(""); }} className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm">
            {Object.entries(ITEM_LABELS).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select>
          <select value={itemId} onChange={(e) => setItemId(e.target.value)} className="min-w-56 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm">
            <option value="">Choose {ITEM_LABELS[itemType].toLowerCase()}…</option>
            {selectedChoices.map((choice) => <option key={choice.id} value={choice.id}>{choice.label}</option>)}
          </select>
          <button type="button" onClick={() => add.mutate()} disabled={!itemId || add.isPending} className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50">{add.isPending ? "Adding…" : "Add"}</button>
        </div>
        {add.isError && <p className="mt-2 text-xs text-red-400">Could not add item: {add.error.message}</p>}
      </div>
      <div className="mt-4 rounded border border-slate-700/80 bg-slate-900/40 p-3">
        <div className="text-xs font-medium uppercase tracking-wide text-slate-500">Apply this set</div>
        <div className="mt-2 flex flex-wrap gap-2">
          <select value={installationId} onChange={(e) => { setInstallationId(e.target.value); setPreview(null); }} className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm">
            <option value="">Choose harness…</option>
            {installations.map((installation) => <option key={installation.id} value={installation.id}>{installation.harness_type}</option>)}
          </select>
          <div className="flex items-center gap-1">
            <select value={mode} onChange={(e) => { setMode(e.target.value); setPreview(null); }} className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm">
              <option value="append">Append (preserve unmanaged)</option>
              <option value="replaceManaged">Replace managed</option>
            </select>
            <HelpTip label="Set apply mode" side="right">
              Append adds the set's items while preserving unmanaged harness
              entries. Replace managed updates or removes only entries CHM
              previously managed for this harness.
            </HelpTip>
          </div>
          <button type="button" onClick={() => previewMutation.mutate()} disabled={!installationId || previewMutation.isPending} className="rounded border border-blue-500 px-3 py-1 text-sm text-blue-300 disabled:opacity-50">{previewMutation.isPending ? "Previewing…" : "Preview"}</button>
        </div>
        {previewMutation.isError && <p className="mt-2 text-xs text-red-400">Preview failed: {previewMutation.error.message}</p>}
        {preview && (
          <div className="mt-3 text-xs text-slate-300">
            <p>
              {preview.summary} · {preview.writableChanges} native file change(s)
              {preview.protectedChanges > 0 &&
                ` · ${preview.protectedChanges} protected credential change${preview.protectedChanges === 1 ? "" : "s"}`}
              {preview.hasBlockers ? " · blockers need review" : ""}
            </p>
            {preview.actions.length > 0 && <ul className="mt-1 list-inside list-disc text-slate-400">{preview.actions.map((action, index) => <li key={`${action.kind}:${action.identity}:${index}`}><span className={action.action === "conflict" || action.action === "unsupported" ? "text-red-300" : undefined}>{action.action}</span> {action.kind} {action.identity}</li>)}</ul>}
            <button type="button" onClick={() => apply.mutate()} disabled={apply.isPending || preview.hasBlockers || (preview.writableChanges === 0 && preview.protectedChanges === 0)} className="mt-2 rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50">{apply.isPending ? "Applying…" : "Apply set"}</button>
            {apply.isError && <p className="mt-2 text-red-400">Apply failed: {apply.error.message}</p>}
          </div>
        )}
      </div>
      {confirmDialog}
    </li>
  );
}

export default function SetsScreen() {
  const qc = useQueryClient();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const sets = useQuery({ queryKey: ["sets"], queryFn: listSets });
  const routes = useQuery({ queryKey: ["routes"], queryFn: listRoutes });
  const mcp = useQuery({ queryKey: ["mcp"], queryFn: listMcp });
  const skills = useQuery({ queryKey: ["skills"], queryFn: listSkills });
  const { data: installations } = useInstallations();
  const create = useMutation({
    mutationFn: () => createSet(name.trim(), description.trim() || null),
    onSuccess: () => { setName(""); setDescription(""); setShowCreate(false); void qc.invalidateQueries({ queryKey: ["sets"] }); },
  });
  const choices = useMemo(() => ({
    model_route: (routes.data ?? []).map((r) => ({ id: r.id, label: `${r.display_name} · ${r.provider_name}` })),
    mcp_server: (mcp.data ?? []).map((s) => ({ id: s.id, label: s.name })),
    skill: (skills.data ?? []).map((s) => ({ id: s.id, label: s.name })),
  }), [routes.data, mcp.data, skills.data]);

  return (
    <div>
      <div className="flex items-center justify-between"><div><h1 className="text-2xl font-bold">Configuration sets</h1><p className="mt-1 text-sm text-slate-400">Reusable bundles of models, MCP servers, and skills. Preview before applying to a harness.</p></div><button type="button" onClick={() => setShowCreate((v) => !v)} className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500">{showCreate ? "Cancel" : "Create set"}</button></div>
      {showCreate && <form className="mt-4 rounded border border-slate-700 bg-slate-800 p-4" onSubmit={(event) => { event.preventDefault(); if (name.trim()) create.mutate(); }}><div className="grid gap-2 md:grid-cols-2"><input required value={name} onChange={(e) => setName(e.target.value)} placeholder="set name" className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" /><input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="description (optional)" className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" /></div><button type="submit" disabled={!name.trim() || create.isPending} className="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50">{create.isPending ? "Creating…" : "Create set"}</button>{create.isError && <p className="mt-2 text-xs text-red-400">Create failed: {create.error.message}</p>}</form>}
      {sets.isLoading ? <p className="mt-4 text-sm text-slate-400">Loading sets…</p> : <ul className="mt-4 space-y-3">{(sets.data ?? []).map((set) => <SetCard key={set.id} set={set} choices={choices} installations={installations ?? []} />)}</ul>}
      {!sets.isLoading && (sets.data ?? []).length === 0 && <p className="mt-5 text-sm text-slate-500">No sets yet. Create one to bundle a repeatable harness setup.</p>}
    </div>
  );
}
