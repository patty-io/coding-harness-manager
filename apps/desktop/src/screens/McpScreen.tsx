// MCP screen: registry table, add form, bindings, diagnostics.

import { useState } from "react";
import { useConfirm } from "../components/ConfirmDialog";
import { useQuery } from "@tanstack/react-query";
import { useCreateMcp, useDeleteMcp, useMcpServers, useRunDiagnostics } from "../hooks/useMcp";
import { SyncToHarnessButton } from "../components/SyncToHarnessButton";
import { detectMcp, type DetectedMcp } from "../lib/api";

function DetectedMcpSection({
  onAdd,
  adding,
}: {
  onAdd: (d: DetectedMcp) => void;
  adding: boolean;
}) {
  const [open, setOpen] = useState(true);
  const { data: detected, isLoading } = useQuery({
    queryKey: ["detected-mcp"],
    queryFn: detectMcp,
    enabled: true,
  });
  const notInLibrary = (detected ?? []).filter((d) => !d.inLibrary);

  return (
    <div className="mt-4 rounded border border-slate-700 bg-slate-800/60 p-4">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between text-left"
      >
        <span className="font-medium text-slate-200">
          Detected from your harnesses
          {notInLibrary.length > 0 && (
            <span className="ml-2 rounded bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300">
              {notInLibrary.length} not in library
            </span>
          )}
        </span>
        <span className="text-xs text-slate-400">{open ? "hide" : "show"}</span>
      </button>
      {open && (
        <>
          {isLoading && <p className="mt-3 text-sm text-slate-400">Reading harness configs…</p>}
          {detected && detected.length === 0 && (
            <p className="mt-3 text-sm text-slate-500">No MCP servers found in any harness config.</p>
          )}
          <ul className="mt-3 space-y-1.5">
            {(detected ?? []).map((d) => (
              <li
                key={`${d.name}|${d.transport}|${d.command ?? d.url ?? ""}`}
                className="flex items-center gap-3 rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm"
              >
                <div className="min-w-0 flex-1">
                  <div className="text-slate-100">
                    {d.name}
                    <span className="ml-2 rounded bg-slate-700 px-1.5 py-0.5 text-xs text-slate-300">
                      {d.transport}
                    </span>
                  </div>
                  <div className="truncate font-mono text-xs text-slate-500">
                    {d.command
                      ? [d.command, ...d.args].join(" ")
                      : d.url}
                  </div>
                  <div className="mt-0.5 text-xs text-slate-500">
                    found in {d.foundIn.join(", ")}
                  </div>
                </div>
                {d.inLibrary ? (
                  <span className="rounded bg-green-500/15 px-2 py-0.5 text-xs text-green-400">
                    in library
                  </span>
                ) : (
                  <button
                    onClick={() => onAdd(d)}
                    disabled={adding}
                    className="rounded border border-blue-500 px-2 py-0.5 text-xs text-blue-300 hover:bg-blue-500/10 disabled:opacity-50"
                  >
                    + Add to library
                  </button>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

export default function McpScreen() {
  const { data: servers, isLoading } = useMcpServers();
  const create = useCreateMcp();
  const del = useDeleteMcp();
  const runDiag = useRunDiagnostics();
  const { confirm, confirmDialog } = useConfirm();
  const [name, setName] = useState("");
  const [transport, setTransport] = useState("stdio");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [url, setUrl] = useState("");
  const [lastResults, setLastResults] = useState<Record<string, { check: string; passed: boolean; detail: string }[]>>({});
  const [selectedServers, setSelectedServers] = useState<string[]>([]);
  const [showManual, setShowManual] = useState(false);

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
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-2xl font-bold">MCP Servers</h1>
        <div className="flex gap-2">
          <SyncToHarnessButton
            selection={{ mcpIds: selectedServers }}
            disabled={selectedServers.length === 0}
            label={selectedServers.length ? `Push selected (${selectedServers.length})…` : "Push selected…"}
          />
          <SyncToHarnessButton label="Sync entire library…" />
        </div>
      </div>

      <div className="mt-4">
        <button
          type="button"
          onClick={() => setShowManual((v) => !v)}
          aria-expanded={showManual}
          className="rounded border border-slate-600 px-3 py-1 text-sm text-slate-300 hover:bg-slate-700"
        >
          {showManual ? "Hide manual entry" : "Add manually…"}
        </button>
      </div>

      {showManual && <div className="mt-3 rounded border border-slate-700 bg-slate-800 p-4">
        <h2 className="font-medium">Add MCP server manually</h2>
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
      </div>}

      <DetectedMcpSection
        onAdd={(d) =>
          create.mutate({
            name: d.name,
            transport: d.transport,
            command: d.command,
            args: d.args,
            url: d.url,
            env: {},
          })
        }
        adding={create.isPending}
      />

      {isLoading && <p className="mt-4">Loading…</p>}
      <table className="mt-4 w-full bg-slate-800 text-sm">
        <thead>
          <tr className="border-b text-left">
            <th className="p-2"><span className="sr-only">Select</span></th>
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
              <td className="p-2">
                <input
                  type="checkbox"
                  aria-label={`Select ${s.name}`}
                  checked={selectedServers.includes(s.id)}
                  onChange={() =>
                    setSelectedServers((previous) =>
                      previous.includes(s.id)
                        ? previous.filter((id) => id !== s.id)
                        : [...previous, s.id],
                    )
                  }
                />
              </td>
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
                  <button
                    onClick={() =>
                      confirm(
                        `Delete ${s.name}?`,
                        "This removes the MCP server from your library.",
                        () => del.mutateAsync(s.id).then(() => undefined),
                        "Delete",
                      )
                    }
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
      {confirmDialog}
    </div>
  );
}
