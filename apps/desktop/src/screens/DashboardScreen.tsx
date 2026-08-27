import { Link } from "react-router-dom";
import { useInstallations } from "../hooks/useHarnesses";

export function DashboardScreen() {
  const { data: installations } = useInstallations();
  const harnesses = installations?.length ?? 0;

  return (
    <div>
      <h1 className="text-2xl font-bold">Dashboard</h1>
      <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-4">
        <StatCard label="Harnesses detected" value={harnesses} />
        <StatCard label="Providers" value="—" />
        <StatCard label="My Models" value="—" />
        <StatCard label="MCP Servers" value="—" />
      </div>
      <div className="mt-6">
        <Link
          to="/import"
          className="inline-block rounded bg-blue-600 px-4 py-2 text-white"
        >
          Import existing configuration
        </Link>
      </div>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded border border-gray-200 bg-white p-4">
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-sm text-gray-600">{label}</div>
    </div>
  );
}