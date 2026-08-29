import { NavLink } from "react-router-dom";

type Item = { to: string; label: string };
type Section = { heading: string; items: Item[] };

const SECTIONS: Section[] = [
  {
    heading: "Overview",
    items: [{ to: "/", label: "Dashboard" }],
  },
  {
    heading: "Harnesses",
    items: [{ to: "/harnesses", label: "All harnesses" }],
  },
  {
    heading: "Library",
    items: [
      { to: "/providers", label: "Providers" },
      { to: "/models", label: "Models" },
      { to: "/mcp", label: "MCP Servers" },
      { to: "/skills", label: "Skills" },
      { to: "/profiles", label: "Presets" },
      { to: "/sets", label: "Configuration sets" },
    ],
  },
  {
    heading: "Changes",
    items: [
      { to: "/history", label: "History" },
    ],
  },
  {
    heading: "Support",
    items: [{ to: "/doctor", label: "Doctor" }],
  },
];

export default function Sidebar() {
  return (
    <nav className="flex w-56 shrink-0 flex-col border-r border-slate-700 bg-slate-900 p-3">
      <div className="mb-1 px-2">
        <div className="whitespace-nowrap text-[13px] font-bold tracking-tight text-slate-100">
          Coding Harness Manager
        </div>
        <div className="text-[10px] uppercase tracking-wide text-slate-500">
          configure · preview · sync
        </div>
      </div>
      <div className="flex-1 overflow-auto">
        {SECTIONS.map((section) => (
          <div key={section.heading} className="mt-4">
            <div className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              {section.heading}
            </div>
            {section.items.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.to === "/"}
                aria-label={item.label}
                className={({ isActive }) =>
                  `mb-0.5 block rounded px-2 py-1.5 text-sm ${
                    isActive
                      ? "bg-blue-600/20 font-medium text-blue-300"
                      : "text-slate-300 hover:bg-slate-800 hover:text-slate-100"
                  }`
                }
              >
                {item.label}
              </NavLink>
            ))}
          </div>
        ))}
      </div>
      <div className="space-y-0.5 border-t border-slate-700 pt-3">
        <NavLink
          to="/settings"
          aria-label="Settings"
          className={({ isActive }) =>
            `block rounded px-2 py-1.5 text-sm ${
              isActive
                ? "bg-blue-600/20 font-medium text-blue-300"
                : "text-slate-300 hover:bg-slate-800"
            }`
          }
        >
          Settings
        </NavLink>
        <NavLink
          to="/import"
          aria-label="Import existing setup"
          className={({ isActive }) =>
            `block rounded px-2 py-1.5 text-sm ${
              isActive
                ? "bg-blue-600/20 font-medium text-blue-300"
                : "text-slate-300 hover:bg-slate-800"
            }`
          }
        >
          + Import existing setup
        </NavLink>
      </div>
    </nav>
  );
}
