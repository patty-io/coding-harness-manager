import { useEffect, useState } from "react";
import { useEndpoints, useProviders } from "../hooks/useProviders";
import type { RouteCreateInput } from "../lib/api";
import type { UseMutationResult } from "@tanstack/react-query";
import { Field } from "./Field";

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
    <div className="mt-6 rounded border border-slate-700 bg-slate-800 p-4">
      <h2 className="font-medium">Add Model Manually</h2>
      <div className="mt-2 grid grid-cols-1 gap-2 md:grid-cols-2">
        <Field id="manual-provider" label="Provider" required>
          <select
            value={providerName}
            onChange={(e) => setProviderName(e.target.value)}
            className="w-full rounded border border-slate-600 px-2 py-1"
          >
            <option value="">Choose provider…</option>
            {(allProviders ?? []).map((p) => (
              <option key={p.id} value={p.display_name}>
                {p.display_name}
              </option>
            ))}
          </select>
        </Field>
        <Field id="manual-endpoint" label="Endpoint" required>
          <select
            value={endpointId}
            onChange={(e) => setEndpointId(e.target.value)}
            className="w-full rounded border border-slate-600 px-2 py-1"
          >
            <option value="">Choose endpoint…</option>
            {(endpoints ?? []).map((e) => (
              <option key={e.id} value={e.id}>
                {e.name}
              </option>
            ))}
          </select>
        </Field>
        <Field id="manual-remote-model-id" label="Remote model id" required>
          <input
            value={remoteModelId}
            onChange={(e) => setRemoteModelId(e.target.value)}
            placeholder="e.g. qwen3.8-27b"
            className="w-full rounded border border-slate-600 px-2 py-1"
          />
        </Field>
        <Field id="manual-display-name" label="Display name" description="Optional name shown in the library.">
          <input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="Defaults to remote model id"
            className="w-full rounded border border-slate-600 px-2 py-1"
          />
        </Field>
        <Field id="manual-context-window" label="Context window" description="Optional token limit.">
          <input
            value={contextWindow}
            onChange={(e) => setContextWindow(e.target.value)}
            placeholder="e.g. 128000"
            type="number"
            min="1"
            className="w-full rounded border border-slate-600 px-2 py-1"
          />
        </Field>
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
          className="rounded border border-slate-600 px-3 py-1 text-sm"
        >
          Cancel
        </button>
      </div>
      {create.isError && (
        <p className="mt-2 text-sm text-red-600">{create.error.message}</p>
      )}
      {providers.length === 0 && (
        <p className="mt-2 text-xs text-slate-400">
          Add a provider with an endpoint first.
        </p>
      )}
    </div>
  );
}
