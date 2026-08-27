import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  useCreateEndpoint,
  useEndpoints,
  useProviderSummary,
  useProviders,
  useDiscoverProvider,
  useSaveApiKey,
  useEnvVarSet,
} from "../hooks/useProviders";
import { EndpointActions } from "../components/EndpointActions";
import type { ProviderDiscoverReport } from "../lib/api";

const PROTOCOLS = [
  { value: "anthropic-messages", label: "Anthropic Messages compatible" },
  { value: "openai-chat", label: "OpenAI Chat Completions compatible" },
  { value: "openai-responses", label: "OpenAI Responses compatible" },
  { value: "openrouter-openai", label: "OpenRouter-style OpenAI compatible" },
  { value: "custom", label: "Custom / unknown" },
];

const PROTOCOL_LABEL: Record<string, string> = Object.fromEntries(
  PROTOCOLS.map((p) => [p.value, p.label]),
);

export default function ProviderDetailScreen() {
  const { id } = useParams<{ id: string }>();
  const { data: providers } = useProviders();
  const { data: endpoints, isLoading: endpointsLoading } = useEndpoints(id);
  const { data: summary } = useProviderSummary(id);
  const create = useCreateEndpoint();
  const saveKey = useSaveApiKey();
  const envSet = useEnvVarSet();

  const provider = (providers ?? []).find((p) => p.id === id);

  const discoverAll = useDiscoverProvider(id);
  const [discoverResult, setDiscoverResult] = useState<ProviderDiscoverReport | null>(null);

  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [protocol, setProtocol] = useState("anthropic-messages");
  const [credentialSource, setCredentialSource] = useState<
    "keychain" | "env" | "none"
  >("env");
  const [envVarName, setEnvVarName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [envWarning, setEnvWarning] = useState<string | null>(null);
  const [savedNote, setSavedNote] = useState<string | null>(null);

  const submitEndpoint = async () => {
    if (!name.trim() || !baseUrl.trim()) return;
    setSavedNote(null);
    setEnvWarning(null);
    let credentialRefId: string | null = null;
    let pendingEnvVar: string | undefined = undefined;
    if (credentialSource === "keychain") {
      if (!apiKey) {
        setSavedNote("Enter an API key for keychain storage");
        return;
      }
      credentialRefId = await saveKey.mutateAsync({
        keyName: `${name.trim()}-${Date.now()}`,
        value: apiKey,
      });
      setSavedNote("Saved to macOS Keychain");
    } else if (credentialSource === "env") {
      if (!envVarName.trim()) {
        setSavedNote("Enter an env var name for env references");
        return;
      }
      const set = await envSet.mutateAsync(envVarName.trim());
      if (!set) {
        setEnvWarning(
          `Environment variable ${envVarName.trim()} is not currently set — validation will fail until it is exported.`,
        );
      }
      pendingEnvVar = envVarName.trim();
    }
    create.mutate(
      {
        input: {
          providerId: id!,
          name: name.trim(),
          baseUrl: baseUrl.trim(),
          protocol,
          discoveryPath: "/v1/models",
          authType: credentialSource === "none" ? "none" : "bearer-token",
          credentialRefId,
          headers: {},
          enabled: true,
        },
        envVarName: pendingEnvVar,
      },
      {
        onSuccess: () => {
          setShowForm(false);
          setName("");
          setBaseUrl("");
          setApiKey("");
          setEnvVarName("");
        },
      },
    );
  };

  return (
    <div>
      <Link
        to="/providers"
        className="inline-flex items-center gap-1 text-sm text-slate-400 hover:text-slate-200"
      >
        ← Providers
      </Link>

      <div className="mt-2 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">
            {provider?.display_name ?? "…"}
          </h1>
          <p className="mt-0.5 font-mono text-xs text-slate-500">
            {provider?.name}
            {provider?.notes ? ` · ${provider.notes}` : ""}
          </p>
        </div>
        {provider && (
          <span
            className={`rounded px-2 py-0.5 text-xs ${
              provider.enabled
                ? "bg-green-500/15 text-green-400"
                : "bg-slate-700 text-slate-400"
            }`}
          >
            {provider.enabled ? "enabled" : "disabled"}
          </span>
        )}
      </div>

      {summary && (
        <div className="mt-3 flex gap-4 text-sm text-slate-400">
          <span>
            <strong className="text-slate-200">{summary.endpoints}</strong>{" "}
            endpoints
          </span>
          <span>
            <strong className="text-slate-200">
              {summary.discoveredModels}
            </strong>{" "}
            discovered models
          </span>
          <span>
            <strong className="text-slate-200">{summary.myModels}</strong> My
            Models
          </span>
        </div>
      )}

      <div className="mt-6 flex items-center justify-between">
        <h2 className="font-medium text-slate-200">Endpoints</h2>
        <div className="flex items-center gap-2">
          <button
            onClick={() =>
              discoverAll.mutate(undefined, { onSuccess: setDiscoverResult })
            }
            disabled={discoverAll.isPending || (endpoints ?? []).length === 0}
            className="rounded border border-blue-500 px-3 py-1 text-sm text-blue-300 hover:bg-blue-500/10 disabled:opacity-50"
          >
            {discoverAll.isPending ? "Discovering all…" : "Discover all models"}
          </button>
          <button
            onClick={() => setShowForm((v) => !v)}
            className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500"
          >
            {showForm ? "Cancel" : "+ Add Endpoint"}
          </button>
        </div>
      </div>

      {discoverAll.isError && (
        <p className="mt-2 text-sm text-red-400">
          Discovery failed: {discoverAll.error.message}
        </p>
      )}

      {discoverResult && (
        <div className="mt-3 rounded border border-slate-700 bg-slate-800 p-3 text-sm text-slate-200">
          <div className="flex items-center justify-between">
            <div>
              <strong>{discoverResult.endpoints_succeeded}</strong> /{" "}
              {discoverResult.endpoints_attempted} endpoints reached ·
              {" "}
              <strong>{discoverResult.added}</strong> new,
              {" "}
              <strong>{discoverResult.updated}</strong> updated
              {" "}
              (<strong>{discoverResult.total}</strong> models seen)
            </div>
            <button
              onClick={() => setDiscoverResult(null)}
              className="text-xs text-slate-400 hover:text-slate-200"
            >
              dismiss
            </button>
          </div>
          <ul className="mt-2 space-y-1 text-xs">
            {discoverResult.outcomes.map((o) => (
              <li key={o.endpoint_id} className="flex items-center gap-2">
                <span
                  className={
                    o.error
                      ? "text-red-400"
                      : o.report && o.report.added > 0
                        ? "text-green-400"
                        : "text-slate-400"
                  }
                >
                  {o.error
                    ? "✗"
                    : o.report && o.report.added > 0
                      ? "✓"
                      : "•"}
                </span>
                <span className="font-mono">{o.endpoint_name}</span>
                {o.error ? (
                  <span className="text-red-400">{o.error}</span>
                ) : (
                  <span className="text-slate-500">
                    {o.report?.total} seen, {o.report?.added} new,{" "}
                    {o.report?.updated} updated
                  </span>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}

      {showForm && (
        <div className="mt-3 rounded border border-slate-700 bg-slate-800 p-4">
          <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="endpoint name (e.g. Anthropic-compatible)"
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-slate-200"
            />
            <input
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="base URL (e.g. https://api.z.ai/api/anthropic)"
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-slate-200"
            />
            <select
              value={protocol}
              onChange={(e) => setProtocol(e.target.value)}
              className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-slate-200"
            >
              {PROTOCOLS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
            <div className="flex gap-3 text-sm text-slate-300">
              <label>
                <input
                  type="radio"
                  checked={credentialSource === "env"}
                  onChange={() => setCredentialSource("env")}
                />{" "}
                Env var
              </label>
              <label>
                <input
                  type="radio"
                  checked={credentialSource === "keychain"}
                  onChange={() => setCredentialSource("keychain")}
                />{" "}
                Store on this computer
              </label>
              <label>
                <input
                  type="radio"
                  checked={credentialSource === "none"}
                  onChange={() => setCredentialSource("none")}
                />{" "}
                No auth
              </label>
            </div>
            {credentialSource === "env" && (
              <input
                value={envVarName}
                onChange={(e) => setEnvVarName(e.target.value)}
                placeholder="env var name (e.g. ZAI_API_KEY)"
                className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-slate-200"
              />
            )}
            {credentialSource === "keychain" && (
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="API key (stored in Keychain, never in the DB)"
                className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-slate-200"
              />
            )}
          </div>
          <button
            onClick={submitEndpoint}
            disabled={create.isPending}
            className="mt-3 rounded bg-blue-600 px-4 py-1 text-white disabled:opacity-50"
          >
            {create.isPending ? "Adding…" : "Add Endpoint"}
          </button>
          {savedNote && (
            <p className="mt-2 text-sm text-green-400">{savedNote}</p>
          )}
          {envWarning && (
            <p className="mt-2 text-sm text-amber-400">{envWarning}</p>
          )}
          {create.isError && (
            <p className="mt-2 text-sm text-red-400">
              Failed: {create.error.message}
            </p>
          )}
        </div>
      )}

      {endpointsLoading && <p className="mt-3 text-sm">Loading endpoints…</p>}

      {endpointsLoading === false && (endpoints ?? []).length === 0 && (
        <p className="mt-3 text-sm text-slate-500">
          No endpoints yet — add one above.
        </p>
      )}

      <ul className="mt-3 space-y-3">
        {(endpoints ?? []).map((e) => (
          <li key={e.id} className="rounded border border-slate-700 bg-slate-800 p-4">
            <div className="flex items-center justify-between">
              <span className="font-medium text-slate-100">{e.name}</span>
              <span className="rounded bg-slate-700 px-2 py-0.5 text-xs text-slate-300">
                {PROTOCOL_LABEL[e.protocol] ?? e.protocol}
              </span>
            </div>
            <div className="mt-1 font-mono text-xs text-slate-400">
              {e.base_url}
            </div>
            <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-xs text-slate-500">
              <span>auth: {e.auth_type}</span>
              <span>discovery: {e.discovery_path ?? "—"}</span>
              {e.credential_ref && (
                <span>
                  credential: {e.credential_ref.kind} —{" "}
                  <span className="font-mono">{e.credential_ref.reference}</span>
                </span>
              )}
              <span>{e.enabled ? "enabled" : "disabled"}</span>
            </div>
            <EndpointActions endpointId={e.id} />
          </li>
        ))}
      </ul>
    </div>
  );
}