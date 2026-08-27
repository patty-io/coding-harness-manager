import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";

export default function InventoryScreen() {
  const { data: installations, isLoading } = useInstallations();
  const scan = useScanHarnesses();

  return (
    <div>
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Harnesses</h1>
        <button
          onClick={() => scan.mutate()}
          disabled={scan.isPending}
          className="rounded bg-blue-600 px-4 py-2 text-white disabled:opacity-50"
        >
          {scan.isPending ? "Scanning..." : "Scan Harnesses"}
        </button>
      </div>
      {scan.isError && (
        <p className="mt-2 text-red-600">Scan failed: {scan.error.message}</p>
      )}
      {isLoading && <p className="mt-4">Loading…</p>}
      <table className="mt-4 w-full bg-white">
        <thead>
          <tr className="border-b text-left">
            <th className="p-2">Harness</th>
            <th className="p-2">Status</th>
            <th className="p-2">Version</th>
            <th className="p-2">Config</th>
          </tr>
        </thead>
        <tbody>
          {(installations ?? []).map((i) => (
            <tr key={i.id} className="border-b">
              <td className="p-2 font-medium">{i.harness_type}</td>
              <td className="p-2">{i.status}</td>
              <td className="p-2">{i.version ?? "—"}</td>
              <td className="p-2 font-mono text-xs">{i.config_path ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}