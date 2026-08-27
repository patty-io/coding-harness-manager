// MCP screen: registry table, add form, bindings, diagnostics.

import { useState } from "react";
import { useCreateMcp, useDeleteMcp, useMcpServers, useRunDiagnostics } from "../hooks/useMcp";
import { SyncToHarnessButton } from "../components/SyncToHarnessButton";

export default function McpScreen() {
  const { data: servers, isLoading } = useMcpServers();
  const create = useCreateMcp();
  const del = useDeleteMcp();
  const runDiag = useRunDiagnostics();
  const [name, setName] = useState("");
  const [transport, setTransport] = useState("stdio");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [url, setUrl] = useState("");
  const [lastResults, setLastResults] = useState<Record<string, { check: string; passed: boolean; detail: string }[]>>({});

  const submit = () => {
    if (!name.trim()) return;
    const parsedArgs = args
      .split("\n")
      .map((a) => a.trim())
      .filter(Boolean);
    create.mutate(
      {
        name: name.trim(),
        transport,
        command: command.trim() || null,
        args: parsedArgs,
        url: url.trim() || null,
        env: {},
      },
      {
        onSuccess: () => {
          setName("");
          setCommand("");
          setArgs("");
          setUrl("");
        },
      },
    );
  };

  return (
    <div>
      <h1 className="text-2xl font-bold">MCP Servers</h1>

      <div className="mt-4 rounded border border-slate-700 bg-slate-800 p-4">
        <h2 className="font-medium">Add MCP Server</h2>
        <div className="mt-2 grid grid-cols-1 gap-2 md:grid-cols-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="name (e.g. github)"
            className="rounded border border-slate-600 px-2 py-1"
          />
          <select
            value={transport}
            onChange={(e) => setTransport(e.target.value)}
            className="rounded border border-slate-600 px-2 py-1"
          >
            <option value="stdio">stdio</option>
            <option value="http">http</option>
            <option value="sse">sse</option>
          </select>
          {transport === "stdio" ? (
            <>
              <input
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                placeholder="command (e.g. npx)"
                className="rounded border border-slate-600 px-2 py-1"
              />
              <textarea
                value={args}
                onChange={(e) => setArgs(e.target.value)}
                placeholder="args (one per line)"
                rows={2}
                className="rounded border border-slate-600 px-2 py-1"
              />
            </>
          ) : (
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="url (e.g. https://mcp.example.com/mcp)"
              className="rounded border border-slate-600 px-2 py-1"
            />
          )}
        </div>
        <button
          onClick={submit}
          disabled={create.isPending || !name.trim()}
          className="mt-3 rounded bg-blue-600 px-4 py-1 text-white disabled:opacity-50"
        >
          {create.isPending ? "Adding…" : "Add MCP Server"}
        </button>
        {create.isError && (
          <p className="mt-2 text-red-600">
            {create.error.message} (name must be unique)
          </p>
        )}
      </div>

      {isLoading && <p className="mt-4">Loading…</p>}
      <table className="mt-4 w-full bg-slate-800 text-sm">
        <thead>
          <tr className="border-b text-left">
            <th className="p-2">Name</th>
            <th className="p-2">Transport</th>
            <th className="p-2">Command / URL</th>
            <th className="p-2">Diagnostics</th>
            <th className="p-2">Actions</th>
          </tr>
        </thead>
        <tbody>
          {(servers ?? []).map((s) => (
            <tr key={s.id} className="border-b">
              <td className="p-2 font-medium">{s.name}</td>
              <td className="p-2">{s.transport}</td>
              <td className="p-2 font-mono text-xs">
                {s.command ?? s.url}
                {s.args.length > 0 && (
                  <div className="text-slate-400">{s.args.join(" ")}</div>
                )}
              </td>
              <td className="p-2">
                <button
                  onClick={() =>
                    runDiag.mutate(s.id, {
                      onSuccess: (results) =>
                        setLastResults((prev) => ({ ...prev, [s.id]: results })),
                    })
                  }
                  disabled={runDiag.isPending}
                  className="rounded border border-slate-600 px-2 py-0.5 text-xs disabled:opacity-50"
                >
                  Run Diagnostics
                </button>
                {lastResults[s.id] && (
                  <ul className="mt-1 text-xs">
                    {lastResults[s.id].map((r) => (
                      <li key={r.check}>
                        <span className={r.passed ? "text-green-700" : "text-red-600"}>
                          {r.passed ? "✓" : "✗"}
                        </span>{" "}
                        {r.check}: {r.detail}
                      </li>
                    ))}
                  </ul>
                )}
              </td>
              <td className="p-2">
                <div className="flex items-center gap-2">
                  <SyncToHarnessButton />
                  <button
                    onClick={() => {
                      if (window.confirm(`Delete MCP server "${s.name}"?`)) {
                        del.mutate(s.id);
                      }
                    }}
                    className="rounded border border-red-200 px-2 py-0.5 text-xs text-red-600"
                  >
                    Delete
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}