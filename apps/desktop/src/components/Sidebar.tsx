import { NavLink } from "react-router-dom";

const NAV_ITEMS = [
  { to: "/", label: "Dashboard" },
  { to: "/providers", label: "Providers" },
  { to: "/models", label: "Models" },
  { to: "/scan", label: "Harnesses" },
  { to: "/mcp", label: "MCP Servers" },
  { to: "/skills", label: "Skills" },
  { to: "/profiles", label: "Profiles" },
  { to: "/sets", label: "Sets" },
  { to: "/changes", label: "Changes" },
  { to: "/history", label: "History" },
  { to: "/doctor", label: "Doctor" },
  { to: "/settings", label: "Settings" },
  { to: "/import", label: "Import Wizard" },
];

export default function Sidebar() {
  return (
    <nav className="flex w-48 shrink-0 flex-col border-r border-slate-700 bg-slate-800 p-3">
      <div className="mb-4 px-2 text-sm font-bold text-slate-100">
        Coding Harness Manager
      </div>
      {NAV_ITEMS.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          end={item.to === "/"}
          className={({ isActive }) =>
            `rounded px-2 py-1.5 text-sm ${
              isActive
                ? "bg-blue-950 font-medium text-blue-700"
                : "text-slate-200 hover:bg-slate-700"
            }`
          }
        >
          {item.label}
        </NavLink>
      ))}
    </nav>
  );
}