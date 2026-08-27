import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  useCreateProvider,
  useDeleteProvider,
  useProviders,
} from "../hooks/useProviders";

export default function ProvidersScreen() {
  const navigate = useNavigate();
  const { data: providers, isLoading } = useProviders();
  const create = useCreateProvider();
  const del = useDeleteProvider();
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");

  const submit = () => {
    if (!name.trim()) return;
    create.mutate(
      { name: name.trim(), displayName: displayName.trim() || name.trim() },
      {
        onSuccess: () => {
          setName("");
          setDisplayName("");
        },
      },
    );
  };

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Providers</h1>
      </div>

      <div className="mt-4 rounded border border-slate-700 bg-slate-800 p-4">
        <h2 className="font-medium">Add Provider</h2>
        <div className="mt-2 flex gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="name (e.g. zai)"
            className="rounded border border-slate-600 px-2 py-1"
          />
          <input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="display name (e.g. Z.AI)"
            className="rounded border border-slate-600 px-2 py-1"
          />
          <button
            onClick={submit}
            disabled={create.isPending || !name.trim()}
            className="rounded bg-blue-600 px-4 py-1 text-white disabled:opacity-50"
          >
            {create.isPending ? "Adding…" : "Add"}
          </button>
        </div>
        {create.isError && (
          <p className="mt-2 text-red-600">
            Failed: {create.error.message} (name must be unique)
          </p>
        )}
      </div>

      {isLoading && <p className="mt-4">Loading…</p>}
      <ul className="mt-4 space-y-2">
        {(providers ?? []).map((p) => (
          <li
            key={p.id}
            className="flex items-center justify-between rounded border border-slate-700 bg-slate-800 p-3"
          >
            <div>
              <Link to={`/providers/${p.id}`} className="font-medium hover:underline">
                {p.display_name}
              </Link>
              <span className="ml-2 text-xs text-slate-400">{p.name}</span>
            </div>
            <div className="flex items-center gap-3" onClick={(e) => e.stopPropagation()}>
              <span
                className={`rounded px-2 py-0.5 text-xs ${
                  p.enabled ? "bg-green-100 text-green-700" : "bg-slate-700 text-slate-300"
                }`}
              >
                {p.enabled ? "enabled" : "disabled"}
              </span>
              <button
                onClick={() => {
                  if (
                    window.confirm(
                      "Delete provider and ALL its endpoints, models, and bindings? This cannot be undone.",
                    )
                  ) {
                    del.mutate(p.id);
                  }
                }}
                className="rounded border border-red-200 px-2 py-0.5 text-xs text-red-600 hover:bg-red-950"
              >
                Delete
              </button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}