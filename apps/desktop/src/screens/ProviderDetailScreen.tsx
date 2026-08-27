import { useState } from "react";
import {
  useCreateEndpoint,
  useEndpoints,
  useProviderSummary,
  useSaveApiKey,
  useEnvVarSet,
} from "../hooks/useProviders";
import { useParams } from "react-router-dom";

const PROTOCOLS = [
  { value: "anthropic-messages", label: "Anthropic Messages compatible" },
  { value: "openai-chat", label: "OpenAI Chat Completions compatible" },
  { value: "openai-responses", label: "OpenAI Responses compatible" },
  { value: "openrouter-openai", label: "OpenRouter-style OpenAI compatible" },
  { value: "custom", label: "Custom / unknown" },
];

export default function ProviderDetailScreen() {
  const { id } = useParams<{ id: string }>();
  const { data: endpoints } = useEndpoints(id);
  const { data: summary } = useProviderSummary(id);
  const create = useCreateEndpoint();
  const saveKey = useSaveApiKey();
  const envSet = useEnvVarSet();

  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [protocol, setProtocol] = useState("anthropic-messages");
  const [credentialSource, setCredentialSource] = useState<"keychain" | "env" | "none">("env");
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
      const set = await envSet.mutateAsync(envVarName.trim());
      if (!set) {
        setEnvWarning(
          `Environment variable ${envVarName.trim()} is not currently set — validation will fail until it is exported.`,
        );
      }
      if (!envVarName.trim()) {
        setSavedNote("Enter an env var name for env references");
        return;
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
      <h1 className="text-2xl font-bold">Provider</h1>
      {summary && (
        <div className="mt-2 flex gap-4 text-sm text-gray-600">
          <span>{summary.endpoints} endpoints</span>
          <span>{summary.discoveredModels} discovered models</span>
          <span>{summary.myModels} My Models</span>
          <span>health: {summary.health}</span>
        </div>
      )}

      <div className="mt-4 rounded border border-gray-200 bg-white p-4">
        <h2 className="font-medium">Add Endpoint</h2>
        <div className="mt-2 grid grid-cols-1 gap-2 md:grid-cols-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="endpoint name (e.g. Anthropic-compatible)"
            className="rounded border border-gray-300 px-2 py-1"
          />
          <input
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="base URL (e.g. https://api.z.ai/api/anthropic)"
            className="rounded border border-gray-300 px-2 py-1"
          />
          <select
            value={protocol}
            onChange={(e) => setProtocol(e.target.value)}
            className="rounded border border-gray-300 px-2 py-1"
          >
            {PROTOCOLS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
          <div className="flex gap-3 text-sm">
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
              className="rounded border border-gray-300 px-2 py-1"
            />
          )}
          {credentialSource === "keychain" && (
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="API key (stored in Keychain, never in the DB)"
              className="rounded border border-gray-300 px-2 py-1"
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
        {savedNote && <p className="mt-2 text-sm text-green-700">{savedNote}</p>}
        {envWarning && <p className="mt-2 text-sm text-amber-700">{envWarning}</p>}
        {create.isError && (
          <p className="mt-2 text-red-600">Failed: {create.error.message}</p>
        )}
      </div>

      <h2 className="mt-6 font-medium">Endpoints</h2>
      <ul className="mt-2 space-y-2">
        {(endpoints ?? []).map((e) => (
          <li key={e.id} className="rounded border border-gray-200 bg-white p-3">
            <div className="flex items-center justify-between">
              <span className="font-medium">{e.name}</span>
              <span className="rounded bg-gray-100 px-2 py-0.5 text-xs">
                {e.protocol}
              </span>
            </div>
            <div className="mt-1 font-mono text-xs text-gray-600">{e.base_url}</div>
            <div className="mt-1 text-xs text-gray-500">
              auth: {e.auth_type}
              {e.credential_ref && (
                <span className="ml-2">
                  credential: {e.credential_ref.kind} ({e.credential_ref.reference})
                </span>
              )}
            </div>
            <EndpointActions endpointId={e.id} />
          </li>
        ))}
      </ul>
    </div>
  );
}

import { EndpointActions } from "../components/EndpointActions";