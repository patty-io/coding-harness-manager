import { useState } from "react";
import { useCatalog, useCheckHealth } from "../hooks/useProviders";

const HEALTH_COLORS: Record<string, string> = {
  Healthy: "bg-green-500/15 text-green-400",
  AuthFailed: "bg-red-500/15 text-red-400",
  Unreachable: "bg-slate-700 text-slate-300",
  RateLimited: "bg-amber-500/15 text-amber-400",
  DiscoveryUnsupported: "bg-slate-700 text-slate-300",
  MalformedResponse: "bg-red-500/15 text-red-400",
  Unknown: "bg-slate-700 text-slate-300",
};

export function EndpointActions({ endpointId }: { endpointId: string }) {
  const [health, setHealth] = useState<string | null>(null);
  const [showCatalog, setShowCatalog] = useState(false);
  const healthCheck = useCheckHealth(endpointId);
  const catalog = useCatalog(showCatalog ? endpointId : undefined);

  return (
    <div className="mt-2 flex flex-wrap items-center gap-2 text-sm">
      <button
        onClick={() =>
          healthCheck.mutate(undefined, { onSuccess: setHealth })
        }
        disabled={healthCheck.isPending}
        className="rounded border border-slate-600 px-2 py-0.5 text-xs disabled:opacity-50"
      >
        {healthCheck.isPending ? "Checking…" : "Check Health"}
      </button>
      {health && (
        <span className={`rounded px-2 py-0.5 text-xs ${HEALTH_COLORS[health] ?? "bg-slate-700 text-slate-300"}`}>
          {health}
        </span>
      )}
      <button
        onClick={() => setShowCatalog((v) => !v)}
        className="rounded border border-slate-600 px-2 py-0.5 text-xs"
      >
        {showCatalog ? "Hide catalog" : "Show catalog"}
      </button>
      {healthCheck.isError && (
        <span className="text-red-400">{healthCheck.error.message}</span>
      )}
      {showCatalog && (
        <ul className="w-full">
          {catalog.isLoading && <li className="text-xs text-slate-400">Loading…</li>}
          {(catalog.data ?? []).map((m) => (
            <li
              key={m.id}
              className="flex justify-between border-t border-slate-700 py-1 text-xs"
            >
              <span className="font-mono">{m.remote_model_id}</span>
              <span
                className={
                  m.status === "new"
                    ? "text-blue-400"
                    : m.status === "missing"
                      ? "text-red-400"
                      : "text-slate-400"
                }
              >
                {m.status}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}