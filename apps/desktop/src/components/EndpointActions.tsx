import { useState } from "react";
import { useCatalog, useCheckHealth, useDiscover } from "../hooks/useProviders";

const HEALTH_COLORS: Record<string, string> = {
  Healthy: "bg-green-100 text-green-700",
  AuthFailed: "bg-red-100 text-red-700",
  Unreachable: "bg-gray-100 text-gray-600",
  RateLimited: "bg-amber-100 text-amber-700",
  DiscoveryUnsupported: "bg-gray-100 text-gray-600",
  MalformedResponse: "bg-red-100 text-red-700",
  Unknown: "bg-gray-100 text-gray-600",
};

export function EndpointActions({ endpointId }: { endpointId: string }) {
  const [health, setHealth] = useState<string | null>(null);
  const [showCatalog, setShowCatalog] = useState(false);
  const healthCheck = useCheckHealth(endpointId);
  const discover = useDiscover(endpointId);
  const catalog = useCatalog(showCatalog ? endpointId : undefined);

  return (
    <div className="mt-2 flex flex-wrap items-center gap-2 text-sm">
      <button
        onClick={() =>
          healthCheck.mutate(undefined, { onSuccess: setHealth })
        }
        disabled={healthCheck.isPending}
        className="rounded border border-gray-300 px-2 py-0.5 text-xs disabled:opacity-50"
      >
        {healthCheck.isPending ? "Checking…" : "Check Health"}
      </button>
      {health && (
        <span className={`rounded px-2 py-0.5 text-xs ${HEALTH_COLORS[health] ?? "bg-gray-100"}`}>
          {health}
        </span>
      )}
      <button
        onClick={() =>
          discover.mutate(undefined, {
            onSuccess: (r) => setHealth(`${r.added} new, ${r.updated} updated from /models`),
          })
        }
        disabled={discover.isPending}
        className="rounded border border-gray-300 px-2 py-0.5 text-xs disabled:opacity-50"
      >
        {discover.isPending ? "Discovering…" : "Discover Models"}
      </button>
      <button
        onClick={() => setShowCatalog((v) => !v)}
        className="rounded border border-gray-300 px-2 py-0.5 text-xs"
      >
        {showCatalog ? "Hide catalog" : "Catalog"}
      </button>
      {discover.isError && (
        <span className="text-red-600">{discover.error.message}</span>
      )}
      {showCatalog && (
        <ul className="w-full">
          {catalog.isLoading && <li className="text-xs text-gray-500">Loading…</li>}
          {(catalog.data ?? []).map((m) => (
            <li key={m.id} className="flex justify-between border-t border-gray-100 py-1 text-xs">
              <span className="font-mono">{m.remote_model_id}</span>
              <span
                className={
                  m.status === "new"
                    ? "text-blue-600"
                    : m.status === "missing"
                      ? "text-red-600"
                      : "text-gray-500"
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