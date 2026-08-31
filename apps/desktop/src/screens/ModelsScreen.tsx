// Phase 6 My Models screen: tabs (My Models / Discovered), filters, enrichment.

import { useMemo, useState } from "react";
import { useQueries, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { useConfirm } from "../components/ConfirmDialog";
import { HelpTip } from "../components/HelpTip";
import { SyncToHarnessButton } from "../components/SyncToHarnessButton";
import {
  useCatalogAll,
  useCreateRoute,
  useDeleteRoute,
  useEnrich,
  useImportBatch,
  useResolveEnrichment,
  useRoutes,
  useUpdateRoute,
} from "../hooks/useModels";
import { ConflictResolver, type EnrichOutcome } from "../components/ConflictResolver";
import {
  harnessModelsView,
  syncApply,
  syncPreview,
  type HarnessInstallation,
  type ModelRouteView,
} from "../lib/api";
import { useInstallations } from "../hooks/useHarnesses";

type Tab = "mine" | "discovered";

export default function ModelsScreen() {
  const [tab, setTab] = useState<Tab>("mine");
  const [providerFilter, setProviderFilter] = useState("");
  const [endpointFilter, setEndpointFilter] = useState("");
  const [stateFilter, setStateFilter] = useState("");
  const [search, setSearch] = useState("");
  const qc = useQueryClient();
  const { data: routes } = useRoutes();
  const { data: installations, isLoading: installationsLoading } = useInstallations();
  const { data: catalog } = useCatalogAll(tab === "discovered");
  const importBatch = useImportBatch();
  const del = useDeleteRoute();
  const update = useUpdateRoute();
  const create = useCreateRoute();
  const enrich = useEnrich();
  const resolve = useResolveEnrichment();

  const syncableInstallations = useMemo(
    () => (installations ?? []).filter((installation) => installation.status !== "detected"),
    [installations],
  );
  const harnessModelQueries = useQueries({
    queries: syncableInstallations.map((installation) => ({
      queryKey: ["harness-models", installation.id],
      queryFn: () => harnessModelsView(installation.id),
      enabled: tab === "mine",
      staleTime: 15_000,
    })),
  });

  const { confirm, confirmDialog } = useConfirm();
  const [selectedCatalog, setSelectedCatalog] = useState<string[]>([]);
  const [selectedModels, setSelectedModels] = useState<string[]>([]);
  const [enrichOutcome, setEnrichOutcome] = useState<{
    routeId: string;
    outcome: EnrichOutcome;
  } | null>(null);
  const [editingRoute, setEditingRoute] = useState<ModelRouteView | null>(null);
  const [editDisplayName, setEditDisplayName] = useState("");
  const [editContextWindow, setEditContextWindow] = useState("");
  const [editMaxInput, setEditMaxInput] = useState("");
  const [editMaxOutput, setEditMaxOutput] = useState("");
  const [routeEditError, setRouteEditError] = useState<string | null>(null);
  const [enrichError, setEnrichError] = useState<string | null>(null);
  const [routeSyncNote, setRouteSyncNote] = useState<string | null>(null);

  const harnessesByRoute = useMemo(() => {
    const memberships = new Map<string, HarnessInstallation[]>();
    syncableInstallations.forEach((installation, index) => {
      for (const model of harnessModelQueries[index]?.data ?? []) {
        if (!model.libraryRouteId) continue;
        const current = memberships.get(model.libraryRouteId) ?? [];
        if (!current.some((item) => item.id === installation.id)) {
          current.push(installation);
        }
        memberships.set(model.libraryRouteId, current);
      }
    });
    return memberships;
  }, [harnessModelQueries, syncableInstallations]);

  const membershipLoading = harnessModelQueries.some((query) => query.isLoading);
  const membershipUnavailable = harnessModelQueries.some((query) => query.isError);

  const beginRouteEdit = (route: ModelRouteView) => {
    setEditingRoute(route);
    setEditDisplayName(route.display_name);
    setEditContextWindow(route.context_window?.toString() ?? "");
    setEditMaxInput(route.max_input?.toString() ?? "");
    setEditMaxOutput(route.max_output?.toString() ?? "");
    setRouteEditError(null);
  };

  const applyRouteToHarnesses = async (
    routeId: string,
    targets: HarnessInstallation[],
  ) => {
    const selection = { modelIds: [routeId] };
    const failures: string[] = [];
    let updated = 0;
    let unchanged = 0;

    for (const installation of targets) {
      const label = installation.harness_type;
      try {
        const preview = await syncPreview(installation.id, "append", selection);
        if (preview.hasBlockers || preview.warnings.length > 0) {
          const details = [
            ...preview.routeBlockers.map((blocker) => blocker.reason),
            ...preview.warnings,
          ].join("; ");
          failures.push(`${label}: ${details || "preview has blockers"}`);
          continue;
        }
        if (preview.writableChanges === 0 && preview.protectedChanges === 0) {
          unchanged += 1;
          continue;
        }
        await syncApply(
          installation.id,
          "append",
          false,
          preview.planHash,
          selection,
        );
        updated += 1;
      } catch (error) {
        failures.push(
          `${label}: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }

    void qc.invalidateQueries({ queryKey: ["installations"] });
    void qc.invalidateQueries({ queryKey: ["routes"] });
    void qc.invalidateQueries({ queryKey: ["harness-models"] });
    void qc.invalidateQueries({ queryKey: ["harness-state"] });
    void qc.invalidateQueries({ queryKey: ["history"] });
    void qc.invalidateQueries({ queryKey: ["dashboard"] });

    const summary = `${updated} harness${updated === 1 ? "" : "es"} updated${
      unchanged > 0 ? `, ${unchanged} already current` : ""
    }`;
    if (failures.length > 0) {
      setRouteSyncNote(`${summary}. ${failures.join(" · ")}`);
      throw new Error(`${summary}; ${failures.length} failed: ${failures.join(" · ")}`);
    }
    setRouteSyncNote(`${summary}.`);
  };

  const saveRouteEdit = () => {
    if (!editingRoute || !editDisplayName.trim()) {
      setRouteEditError("Display name is required.");
      return;
    }
    if (installationsLoading || membershipLoading) {
      setRouteEditError("Still checking harness membership; try saving again in a moment.");
      return;
    }
    const parseLimit = (value: string, label: string) => {
      if (!value.trim()) return undefined;
      const parsed = Number(value);
      if (!Number.isSafeInteger(parsed) || parsed <= 0) {
        throw new Error(`${label} must be a positive whole number.`);
      }
      return parsed;
    };
    try {
      const routeBeingEdited = editingRoute;
      const affectedHarnesses = harnessesByRoute.get(routeBeingEdited.id) ?? [];
      const membershipWasIncomplete = membershipUnavailable;
      const nextDisplayName = editDisplayName.trim();
      const contextWindow = parseLimit(editContextWindow, "Context window");
      const maxInput = parseLimit(editMaxInput, "Max input");
      const maxOutput = parseLimit(editMaxOutput, "Max output");
      update.mutate(
        {
          id: editingRoute.id,
          input: {
            displayName: editDisplayName.trim(),
            contextWindow,
            maxInput,
            maxOutput,
          },
        },
        {
          onSuccess: () => {
            setEditingRoute(null);
            setRouteEditError(null);
            if (membershipWasIncomplete) {
              setRouteSyncNote(
                "Saved to My Models. CHM could not verify every harness membership, so no harnesses were updated automatically.",
              );
              return;
            }
            if (affectedHarnesses.length === 0) {
              setRouteSyncNote("Saved to My Models. This model is not currently configured on a harness.");
              return;
            }
            const names = affectedHarnesses.map((harness) => harness.harness_type).join(", ");
            setRouteSyncNote(
              `Saved to My Models. No harness changes have been applied yet; choose whether to update ${names}.`,
            );
            confirm(
              `Update ${affectedHarnesses.length} harness${affectedHarnesses.length === 1 ? "" : "es"}?`,
              `Saved “${nextDisplayName}” to My Models. It is currently configured on ${names}. Apply the updated metadata to those harnesses now?`,
              () => applyRouteToHarnesses(routeBeingEdited.id, affectedHarnesses),
              "Update harnesses",
              "Save library only",
            );
          },
          onError: (error) =>
            setRouteEditError(error instanceof Error ? error.message : String(error)),
        },
      );
    } catch (error) {
      setRouteEditError(error instanceof Error ? error.message : String(error));
    }
  };

  const providers = useMemo(
    () => [...new Set((routes ?? []).map((r) => r.provider_name))],
    [routes],
  );

  const endpoints = useMemo(
    () =>
      [...
        new Map(
          [
            ...(routes ?? []).map((r) => [
              r.endpoint_id,
              `${r.provider_name} · ${r.endpoint_name}`,
            ] as const),
            ...(catalog ?? []).map((m) => [
              m.endpoint_id,
              `${m.provider_name} · ${m.endpoint_name}`,
            ] as const),
          ],
        ),
      ],
    [routes, catalog],
  );

  const stateOptions =
    tab === "mine"
      ? ["enabled", "disabled"]
      : [...new Set((catalog ?? []).map((m) => m.status))].sort();

  const filteredRoutes = useMemo(
    () =>
      (routes ?? []).filter(
        (r) =>
          (!providerFilter || r.provider_name === providerFilter) &&
          (!endpointFilter || r.endpoint_id === endpointFilter) &&
          (!stateFilter || (stateFilter === "enabled" ? r.enabled : !r.enabled)) &&
          (!search.trim() ||
            `${r.display_name} ${r.remote_model_id} ${r.provider_name} ${r.endpoint_name}`
              .toLowerCase()
              .includes(search.trim().toLowerCase())),
      ),
    [routes, providerFilter, endpointFilter, stateFilter, search],
  );

  const filteredCatalog = useMemo(
    () =>
      (catalog ?? []).filter(
        (m) =>
          (!providerFilter || m.provider_name === providerFilter) &&
          (!endpointFilter || m.endpoint_id === endpointFilter) &&
          (!stateFilter || m.status === stateFilter) &&
          (!search.trim() ||
            `${m.remote_model_id} ${m.provider_name} ${m.endpoint_name}`
              .toLowerCase()
              .includes(search.trim().toLowerCase())),
      ),
    [catalog, providerFilter, endpointFilter, stateFilter, search],
  );

  const toggleCatalog = (id: string) =>
    setSelectedCatalog((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Models</h1>
        <div className="flex gap-1 rounded border border-slate-600 bg-slate-800 p-0.5 text-sm">
          <TabButton active={tab === "mine"} onClick={() => setTab("mine")}>
            My Models
          </TabButton>
          <TabButton
            active={tab === "discovered"}
            onClick={() => setTab("discovered")}
          >
            Discovered
          </TabButton>
        </div>
      </div>

      {tab === "mine" && (
        <>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
            <SyncToHarnessButton
              selection={{ modelIds: selectedModels }}
              disabled={selectedModels.length === 0}
              label={selectedModels.length ? `Push selected (${selectedModels.length})…` : "Push selected…"}
            />
            <HelpTip label="Push selected" side="right">
              Send only the checked My Models to a harness. The next screen
              lets you choose the harness and review the changes.
            </HelpTip>
            <SyncToHarnessButton label="Sync entire library…" />
            <HelpTip label="Sync entire library" side="right">
              Compare all enabled My Models with a harness. Use Push selected
              when you only want one or a few models.
            </HelpTip>
            <label>
              Search:{" "}
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="model or provider"
                className="w-44 rounded border border-slate-600 px-2 py-1"
              />
            </label>
            <label>
              Provider:{" "}
              <select
                value={providerFilter}
                onChange={(e) => setProviderFilter(e.target.value)}
                className="rounded border border-slate-600 px-2 py-1"
              >
                <option value="">all</option>
                {providers.map((p) => (
                  <option key={p} value={p}>
                    {p}
                </option>
              ))}
              </select>
            </label>
            <label>
              Endpoint:{" "}
              <select
                value={endpointFilter}
                onChange={(e) => setEndpointFilter(e.target.value)}
                className="max-w-56 rounded border border-slate-600 px-2 py-1"
              >
                <option value="">all</option>
                {endpoints.map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              State:{" "}
              <select
                value={stateFilter}
                onChange={(e) => setStateFilter(e.target.value)}
                className="rounded border border-slate-600 px-2 py-1"
              >
                <option value="">all</option>
                {stateOptions.map((state) => (
                  <option key={state} value={state}>
                    {state}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <p className="mt-2 text-xs text-slate-500">
            Edit changes this library entry directly. Match metadata only
            checks the local models.dev catalog for canonical context and
            output limits; it does not contact the provider or change the
            remote model id.
          </p>
          {routeSyncNote && (
            <p className="mt-2 text-xs text-slate-300" role="status" aria-live="polite">
              {routeSyncNote}
            </p>
          )}
          {enrichError && <p className="mt-2 text-xs text-red-400">{enrichError}</p>}
          <table className="mt-3 w-full bg-slate-800 text-sm">
            <thead>
              <tr className="border-b text-left">
                <th className="p-2"><span className="sr-only">Select</span></th>
                <th className="p-2">Provider</th>
                <th className="p-2">Endpoint</th>
                <th className="p-2">Model</th>
                <th className="p-2">Harnesses</th>
                <th className="p-2">Context</th>
                <th className="p-2">Source</th>
                <th className="p-2">State</th>
                <th className="p-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              {filteredRoutes.map((r) => (
                <tr key={r.id} className="border-b">
                  <td className="p-2">
                    <input
                      type="checkbox"
                      aria-label={`Select ${r.display_name}`}
                      checked={selectedModels.includes(r.id)}
                      onChange={() =>
                        setSelectedModels((previous) =>
                          previous.includes(r.id)
                            ? previous.filter((id) => id !== r.id)
                            : [...previous, r.id],
                        )
                      }
                    />
                  </td>
                  <td className="p-2">{r.provider_name}</td>
                  <td className="p-2 text-xs text-slate-400">{r.endpoint_name}</td>
                  <td className="p-2">
                    <div className="font-medium">{r.display_name}</div>
                    <div className="font-mono text-xs text-slate-400">
                      {r.remote_model_id}
                    </div>
                  </td>
                  <td className="p-2 text-xs">
                    {membershipLoading ? (
                      <span className="text-slate-500">checking…</span>
                    ) : membershipUnavailable ? (
                      <span className="text-amber-300" title="One or more harness configurations could not be read">
                        unavailable
                      </span>
                    ) : (harnessesByRoute.get(r.id) ?? []).length > 0 ? (
                      <div className="flex flex-wrap gap-1">
                        {(harnessesByRoute.get(r.id) ?? []).map((harness) => (
                          <Link
                            key={harness.id}
                            to={`/harnesses/${harness.id}`}
                            className="rounded bg-blue-500/10 px-1.5 py-0.5 text-blue-300 hover:bg-blue-500/20 hover:underline"
                          >
                            {harness.harness_type}
                          </Link>
                        ))}
                      </div>
                    ) : (
                      <span className="text-slate-500">—</span>
                    )}
                  </td>
                  <td className="p-2">
                    {r.context_window
                      ? r.context_window.toLocaleString()
                      : "—"}
                  </td>
                  <td className="p-2 text-xs">
                    {String(r.provenance?.source ?? "unknown")}
                  </td>
                  <td className="p-2">
                    <button
                      onClick={() =>
                        update.mutate({
                          id: r.id,
                          input: { enabled: !r.enabled },
                        })
                      }
                      className={`rounded px-2 py-0.5 text-xs ${
                        r.enabled
                          ? "bg-green-100 text-green-700"
                          : "bg-slate-700 text-slate-300"
                      }`}
                    >
                      {r.enabled ? "enabled" : "disabled"}
                    </button>
                  </td>
                  <td className="p-2">
                    <button
                      onClick={() => beginRouteEdit(r)}
                      className="rounded border border-slate-600 px-2 py-0.5 text-xs hover:bg-slate-700"
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => {
                        setEnrichError(null);
                        enrich.mutate(r.id, {
                          onSuccess: (o) => setEnrichOutcome({ routeId: r.id, outcome: o }),
                          onError: (error) =>
                            setEnrichError(error instanceof Error ? error.message : String(error)),
                        });
                      }}
                      disabled={enrich.isPending}
                      title="Match this model id to the local models.dev catalog and fill canonical metadata when a match is found."
                      className="ml-1 rounded border border-slate-600 px-2 py-0.5 text-xs disabled:opacity-50"
                    >
                      Match metadata
                    </button>
                    <HelpTip label="Match metadata" side="left">
                      Look up this model in the public models.dev catalog and
                      propose canonical context limits and capabilities. It
                      does not change the route until you resolve and apply a
                      match.
                    </HelpTip>
                    <button
                      onClick={() =>
                        confirm(
                          "Delete this model route?",
                          "It will no longer be available to sync to harnesses.",
                          () => del.mutateAsync(r.id),
                          "Delete",
                        )
                      }
                      className="ml-1 rounded border border-red-200 px-2 py-0.5 text-xs text-red-600"
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      {tab === "discovered" && (
        <>
          <div className="mt-3 text-sm">
            <button
              onClick={() =>
                importBatch.mutate(selectedCatalog, {
                  onSuccess: () => setSelectedCatalog([]),
                })
              }
              disabled={selectedCatalog.length === 0 || importBatch.isPending}
              className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50"
            >
              Add {selectedCatalog.length > 0 ? `(${selectedCatalog.length})` : ""} to My Models
            </button>
            <span className="ml-3 text-xs text-slate-500">
              Select discovered models by endpoint; status is refreshed by provider discovery.
            </span>
          </div>
          <table className="mt-3 w-full bg-slate-800 text-sm">
            <thead>
              <tr className="border-b text-left">
                <th className="p-2"></th>
                <th className="p-2">Provider</th>
                <th className="p-2">Remote model id</th>
                <th className="p-2">Endpoint</th>
                <th className="p-2">Status</th>
              </tr>
            </thead>
            <tbody>
              {filteredCatalog
                .filter((m) => !m.in_my_models)
                .map((m) => (
                <tr key={m.id} className="border-b">
                  <td className="p-2">
                    <input
                      type="checkbox"
                      checked={selectedCatalog.includes(m.id)}
                      onChange={() => toggleCatalog(m.id)}
                    />
                  </td>
                  <td className="p-2">{m.provider_name}</td>
                  <td className="p-2 font-mono text-xs">{m.remote_model_id}</td>
                  <td className="p-2 text-xs text-slate-400">{m.endpoint_name}</td>
                  <td className="p-2 text-xs">
                    {m.status}
                    {m.match_confidence !== null && (
                      <span className="ml-1 text-slate-400">
                        {m.match_confidence}% match
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      {enrichOutcome && (
        <ConflictResolver
          outcome={enrichOutcome.outcome}
          onResolve={(identityId) => {
            resolve.mutate({
              routeId: enrichOutcome.routeId,
              identityId,
            });
            setEnrichOutcome(null);
          }}
          onClose={() => setEnrichOutcome(null)}
        />
      )}

      {editingRoute && (
        <RouteEditDialog
          route={editingRoute}
          displayName={editDisplayName}
          contextWindow={editContextWindow}
          maxInput={editMaxInput}
          maxOutput={editMaxOutput}
          error={routeEditError}
          saving={update.isPending}
          onDisplayNameChange={setEditDisplayName}
          onContextWindowChange={setEditContextWindow}
          onMaxInputChange={setEditMaxInput}
          onMaxOutputChange={setEditMaxOutput}
          onSave={saveRouteEdit}
          onClose={() => setEditingRoute(null)}
        />
      )}

      <ManualModelForm
        onCreated={() => setTab("mine")}
        create={create}
        providers={providers}
      />
      {confirmDialog}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`rounded px-3 py-1 ${active ? "bg-blue-600 text-white" : "text-slate-200"}`}
    >
      {children}
    </button>
  );
}

function RouteEditDialog({
  route,
  displayName,
  contextWindow,
  maxInput,
  maxOutput,
  error,
  saving,
  onDisplayNameChange,
  onContextWindowChange,
  onMaxInputChange,
  onMaxOutputChange,
  onSave,
  onClose,
}: {
  route: ModelRouteView;
  displayName: string;
  contextWindow: string;
  maxInput: string;
  maxOutput: string;
  error: string | null;
  saving: boolean;
  onDisplayNameChange: (value: string) => void;
  onContextWindowChange: (value: string) => void;
  onMaxInputChange: (value: string) => void;
  onMaxOutputChange: (value: string) => void;
  onSave: () => void;
  onClose: () => void;
}) {
  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="w-full max-w-lg rounded border border-slate-700 bg-slate-800 p-4 shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 className="font-medium text-slate-100">Edit model route</h2>
        <p className="mt-1 text-xs text-slate-400">
          {route.provider_name} · {route.remote_model_id}
        </p>
        <p className="mt-2 text-sm text-slate-400">
          Set the metadata used by profiles and harness sync. This does not
          change the provider or remote model id.
        </p>
        <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="sm:col-span-2">
            <span className="text-xs text-slate-400">Display name</span>
            <input
              value={displayName}
              onChange={(event) => onDisplayNameChange(event.target.value)}
              className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm"
              autoFocus
            />
          </label>
          <label>
            <span className="text-xs text-slate-400">Context window</span>
            <input
              value={contextWindow}
              onChange={(event) => onContextWindowChange(event.target.value)}
              type="number"
              min="1"
              step="1"
              placeholder="e.g. 128000"
              className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm"
            />
          </label>
          <label>
            <span className="text-xs text-slate-400">Max output</span>
            <input
              value={maxOutput}
              onChange={(event) => onMaxOutputChange(event.target.value)}
              type="number"
              min="1"
              step="1"
              placeholder="optional"
              className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm"
            />
          </label>
          <label>
            <span className="text-xs text-slate-400">Max input</span>
            <input
              value={maxInput}
              onChange={(event) => onMaxInputChange(event.target.value)}
              type="number"
              min="1"
              step="1"
              placeholder="optional"
              className="mt-1 w-full rounded border border-slate-600 bg-slate-900 px-2 py-1.5 text-sm"
            />
          </label>
        </div>
        {error && <p className="mt-3 text-xs text-red-400">{error}</p>}
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
          >
            Cancel
          </button>
          <button
            onClick={onSave}
            disabled={saving}
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500 disabled:opacity-50"
          >
            {saving ? "Saving…" : "Save changes"}
          </button>
        </div>
      </div>
    </div>
  );
}

import { ManualModelForm } from "../components/ManualModelForm";
