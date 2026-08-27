import { useEffect, useState } from "react";
import { useEndpoints, useProviders } from "../hooks/useProviders";
import type { RouteCreateInput } from "../lib/api";
import type { UseMutationResult } from "@tanstack/react-query";

export function ManualModelForm({
  create,
  providers,
  onCreated,
}: {
  create: UseMutationResult<string, Error, RouteCreateInput, unknown>;
  providers: string[];
  onCreated: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [providerName, setProviderName] = useState("");
  const [endpointId, setEndpointId] = useState("");
  const [remoteModelId, setRemoteModelId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [contextWindow, setContextWindow] = useState("");
  const { data: allProviders } = useProviders();
  const { data: endpoints } = useEndpoints(
    allProviders?.find((p) => p.display_name === providerName)?.id,
  );

  useEffect(() => {
    setEndpointId(endpoints?.[0]?.id ?? "");
  }, [endpoints]);

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="mt-6 rounded bg-blue-600 px-3 py-1 text-sm text-white"
      >
        Add Model Manually
      </button>
    );
  }

  const submit = () => {
    if (!remoteModelId.trim() || !endpointId) return;
    create.mutate(
      {
        endpointId,
        remoteModelId: remoteModelId.trim(),
        displayName: displayName.trim() || remoteModelId.trim(),
        contextWindow: contextWindow ? Number(contextWindow) : undefined,
      },
      {
        onSuccess: () => {
          setOpen(false);
          setRemoteModelId("");
          setDisplayName("");
          setContextWindow("");
          onCreated();
        },
      },
    );
  };

  return (
    <div className="mt-6 rounded border border-gray-200 bg-white p-4">
      <h2 className="font-medium">Add Model Manually</h2>
      <div className="mt-2 grid grid-cols-1 gap-2 md:grid-cols-2">
        <select
          value={providerName}
          onChange={(e) => setProviderName(e.target.value)}
          className="rounded border border-gray-300 px-2 py-1"
        >
          <option value="">provider…</option>
          {(allProviders ?? []).map((p) => (
            <option key={p.id} value={p.display_name}>
              {p.display_name}
            </option>
          ))}
        </select>
        <select
          value={endpointId}
          onChange={(e) => setEndpointId(e.target.value)}
          className="rounded border border-gray-300 px-2 py-1"
        >
          {(endpoints ?? []).map((e) => (
            <option key={e.id} value={e.id}>
              {e.name}
            </option>
          ))}
        </select>
        <input
          value={remoteModelId}
          onChange={(e) => setRemoteModelId(e.target.value)}
          placeholder="remote model id (required)"
          className="rounded border border-gray-300 px-2 py-1"
        />
        <input
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          placeholder="display name"
          className="rounded border border-gray-300 px-2 py-1"
        />
        <input
          value={contextWindow}
          onChange={(e) => setContextWindow(e.target.value)}
          placeholder="context window"
          type="number"
          className="rounded border border-gray-300 px-2 py-1"
        />
      </div>
      <div className="mt-3 flex gap-2">
        <button
          onClick={submit}
          disabled={create.isPending || !remoteModelId.trim() || !endpointId}
          className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
        >
          {create.isPending ? "Creating…" : "Create Route"}
        </button>
        <button
          onClick={() => setOpen(false)}
          className="rounded border border-gray-300 px-3 py-1 text-sm"
        >
          Cancel
        </button>
      </div>
      {create.isError && (
        <p className="mt-2 text-sm text-red-600">{create.error.message}</p>
      )}
      {providers.length === 0 && (
        <p className="mt-2 text-xs text-gray-500">
          Add a provider with an endpoint first.
        </p>
      )}
    </div>
  );
}