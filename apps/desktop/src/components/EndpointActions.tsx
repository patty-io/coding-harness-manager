import { useState } from "react";
import { useCheckHealth, useDiscover } from "../hooks/useProviders";

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
  const healthCheck = useCheckHealth(endpointId);
  const discover = useDiscover(endpointId);

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
      <button
        onClick={() => discover.mutate()}
        disabled={discover.isPending}
        className="rounded border border-slate-600 px-2 py-0.5 text-xs disabled:opacity-50"
      >
        {discover.isPending ? "Discovering…" : "Discover models"}
      </button>
      {health && (
        <span className={`rounded px-2 py-0.5 text-xs ${HEALTH_COLORS[health] ?? "bg-slate-700 text-slate-300"}`}>
          {health}
        </span>
      )}
      {healthCheck.isError && (
        <span className="text-red-400">{healthCheck.error.message}</span>
      )}
      {discover.data && (
        <span className="text-xs text-slate-400">
          {discover.data.added} new · {discover.data.updated} updated
        </span>
      )}
      {discover.isError && (
        <span className="text-red-400">{discover.error.message}</span>
      )}
    </div>
  );
}
