import { Link } from "react-router-dom";
import { useDashboardStats } from "../hooks/useDashboard";

export function DashboardScreen() {
  const { data: stats } = useDashboardStats();

  return (
    <div>
      <h1 className="text-2xl font-bold">Dashboard</h1>
      <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-3">
        <StatCard label="Harnesses detected" value={stats?.harnesses ?? "—"} />
        <StatCard label="Providers" value={stats?.providers ?? "—"} />
        <StatCard label="My Models" value={stats?.models ?? "—"} />
        <StatCard label="MCP Servers" value={stats?.mcp ?? "—"} />
        <StatCard label="Skills" value={stats?.skills ?? "—"} />
        <StatCard label="Harnesses with drift" value={stats?.drifted ?? "—"} />
      </div>
      <div className="mt-6 space-x-3">
        <Link
          to="/import"
          className="inline-block rounded bg-blue-600 px-4 py-2 text-white"
        >
          Import existing configuration
        </Link>
        <Link
          to="/scan"
          className="inline-block rounded border border-gray-300 bg-white px-4 py-2 text-gray-700"
        >
          Scan Harnesses
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
