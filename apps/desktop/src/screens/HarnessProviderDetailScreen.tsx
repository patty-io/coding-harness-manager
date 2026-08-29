import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";
import { harnessProviderDetail, ensureProviderFromHarness } from "../lib/api";
import { useConfirm } from "../components/ConfirmDialog";

export default function HarnessProviderDetailScreen() {
  const { id, providerName } = useParams<{ id: string; providerName: string }>();
  const decodedName = providerName ? decodeURIComponent(providerName) : "";
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { confirm, confirmDialog } = useConfirm();
  const detail = useQuery({
    queryKey: ["harness-provider-detail", id, decodedName],
    queryFn: () => harnessProviderDetail(id!, decodedName),
    enabled: !!id && !!decodedName,
  });
  const add = useMutation({
    mutationFn: () => ensureProviderFromHarness(id!, decodedName),
    onSuccess: (result) => {
      void qc.invalidateQueries({ queryKey: ["providers"] });
      navigate(`/providers/${result.providerId}`);
    },
  });

  if (detail.isLoading) return <p>Loading harness provider…</p>;
  if (detail.isError) {
    return (
      <div>
        <Link to={`/harnesses/${id}`} className="text-sm text-slate-400 hover:text-slate-200">← Harness</Link>
        <p className="mt-4 rounded border border-red-500/30 bg-red-500/10 p-4 text-sm text-red-300">
          Could not read this provider: {String(detail.error)}
        </p>
      </div>
    );
  }
  const p = detail.data!;
  return (
    <div>
      <Link to={`/harnesses/${id}`} className="text-sm text-slate-400 hover:text-slate-200">← Harness</Link>
      <div className="mt-4 flex items-start justify-between gap-4">
        <div>
          <p className="text-xs uppercase tracking-wide text-slate-500">Harness provider</p>
          <h1 className="text-2xl font-bold text-slate-100">{p.providerName}</h1>
          <p className="mt-1 text-sm text-slate-400">Read-only details from the {p.harnessType} configuration.</p>
        </div>
        <button
          onClick={() => confirm(
            `Add ${p.providerName} to Providers?`,
            `This creates a registry provider and API endpoint using ${p.baseUrl ?? "the harness configuration"}.`,
            () => add.mutateAsync().then(() => undefined),
            "Add to Providers",
          )}
          disabled={add.isPending}
          className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white hover:bg-blue-500 disabled:opacity-50"
        >
          {add.isPending ? "Adding…" : "Add to Providers"}
        </button>
      </div>
      {add.isError && <p className="mt-3 text-sm text-red-400">{String(add.error)}</p>}
      <div className="mt-6 rounded border border-slate-700 bg-slate-800 p-4">
        <dl className="space-y-2 text-sm">
          <div className="flex gap-4"><dt className="w-36 text-slate-500">Base URL</dt><dd className="font-mono text-slate-200">{p.baseUrl ?? "—"}</dd></div>
          <div className="flex gap-4"><dt className="w-36 text-slate-500">Source</dt><dd className="text-slate-200">{p.attributionConfidence}</dd></div>
          <div className="flex gap-4"><dt className="w-36 text-slate-500">Models</dt><dd className="text-slate-200">{p.models.length}</dd></div>
        </dl>
      </div>
      <h2 className="mt-6 text-sm font-medium text-slate-300">Declared models</h2>
      <ul className="mt-2 space-y-1 text-sm text-slate-300">
        {p.models.map((model) => <li key={model} className="rounded border border-slate-700 bg-slate-800 px-3 py-2 font-mono">{model}</li>)}
      </ul>
      {confirmDialog}
    </div>
  );
}
