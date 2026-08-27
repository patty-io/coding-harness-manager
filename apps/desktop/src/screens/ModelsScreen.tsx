// Phase 6 My Models screen: tabs (My Models / Discovered), filters, enrichment.

import { useMemo, useState } from "react";
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

type Tab = "mine" | "discovered";

export default function ModelsScreen() {
  const [tab, setTab] = useState<Tab>("mine");
  const [providerFilter, setProviderFilter] = useState("");
  const { data: routes } = useRoutes();
  const { data: catalog } = useCatalogAll(tab === "discovered");
  const importBatch = useImportBatch();
  const del = useDeleteRoute();
  const update = useUpdateRoute();
  const create = useCreateRoute();
  const enrich = useEnrich();
  const resolve = useResolveEnrichment();

  const [selectedCatalog, setSelectedCatalog] = useState<string[]>([]);
  const [enrichOutcome, setEnrichOutcome] = useState<{
    routeId: string;
    outcome: EnrichOutcome;
  } | null>(null);

  const providers = useMemo(
    () => [...new Set((routes ?? []).map((r) => r.provider_name))],
    [routes],
  );

  const filteredRoutes = useMemo(
    () =>
      (routes ?? []).filter(
        (r) => !providerFilter || r.provider_name === providerFilter,
      ),
    [routes, providerFilter],
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
          <div className="mt-3 flex items-center gap-2 text-sm">
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
          </div>
          <table className="mt-3 w-full bg-slate-800 text-sm">
            <thead>
              <tr className="border-b text-left">
                <th className="p-2">Provider</th>
                <th className="p-2">Model</th>
                <th className="p-2">Context</th>
                <th className="p-2">Source</th>
                <th className="p-2">State</th>
                <th className="p-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              {filteredRoutes.map((r) => (
                <tr key={r.id} className="border-b">
                  <td className="p-2">{r.provider_name}</td>
                  <td className="p-2">
                    <div className="font-medium">{r.display_name}</div>
                    <div className="font-mono text-xs text-slate-400">
                      {r.remote_model_id}
                    </div>
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
                    <SyncToHarnessButton />
                    <button
                      onClick={() =>
                        enrich.mutate(r.id, {
                          onSuccess: (o) => setEnrichOutcome({ routeId: r.id, outcome: o }),
                        })
                      }
                      disabled={enrich.isPending}
                      className="ml-1 rounded border border-slate-600 px-2 py-0.5 text-xs disabled:opacity-50"
                    >
                      Enrich
                    </button>
                    <button
                      onClick={() => {
                        if (window.confirm("Delete this model route?")) {
                          del.mutate(r.id);
                        }
                      }}
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
          </div>
          <table className="mt-3 w-full bg-slate-800 text-sm">
            <thead>
              <tr className="border-b text-left">
                <th className="p-2"></th>
                <th className="p-2">Provider</th>
                <th className="p-2">Remote model id</th>
                <th className="p-2">Status</th>
              </tr>
            </thead>
            <tbody>
              {(catalog ?? []).map((m) => (
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
                  <td className="p-2 text-xs">
                    {m.status}
                    {m.matchConfidence !== null && (
                      <span className="ml-1 text-slate-400">
                        {m.matchConfidence}% match
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

      <ManualModelForm
        onCreated={() => setTab("mine")}
        create={create}
        providers={providers}
      />
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

import { ManualModelForm } from "../components/ManualModelForm";