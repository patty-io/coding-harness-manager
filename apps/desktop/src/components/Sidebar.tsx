import { NavLink } from "react-router-dom";
import productSymbol from "../../../../resources/logos/symbol-ui.svg";

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
      { to: "/profiles", label: "Profiles" },
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

export default function Sidebar({ profilesAndSetsEnabled }: { profilesAndSetsEnabled: boolean }) {
  const sections = SECTIONS.map((section) => ({
    ...section,
    items: section.items.filter(
      (item) => profilesAndSetsEnabled || !["/profiles", "/sets"].includes(item.to),
    ),
  })).filter((section) => section.items.length > 0);

  return (
    <nav className="flex min-h-0 w-56 shrink-0 flex-col border-r border-slate-700 bg-slate-900 p-3">
      <div className="mb-1 flex items-center gap-2 px-2">
        <img src={productSymbol} alt="" className="h-7 w-7 shrink-0" />
        <div className="min-w-0">
          <div className="whitespace-nowrap text-xs font-bold tracking-tight text-slate-100">
            Coding Harness Manager
          </div>
          <div className="whitespace-nowrap text-[9px] tracking-[0.06em] text-slate-500">
            configure · preview · sync
          </div>
        </div>
      </div>
      <div className="flex-1 overflow-auto">
        {sections.map((section) => (
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
                  `mb-0.5 block border-l-2 px-2 py-1.5 text-sm ${
                    isActive
                      ? "border-blue-600 bg-slate-800 font-semibold text-slate-100"
                      : "border-transparent text-slate-300 hover:border-slate-600 hover:bg-slate-800 hover:text-slate-100"
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
            `block border-l-2 px-2 py-1.5 text-sm ${
              isActive
                ? "border-blue-600 bg-slate-800 font-semibold text-slate-100"
                : "border-transparent text-slate-300 hover:border-slate-600 hover:bg-slate-800 hover:text-slate-100"
            }`
          }
        >
          Settings
        </NavLink>
        <NavLink
          to="/import"
          aria-label="Import existing setup"
          className={({ isActive }) =>
            `block border-l-2 px-2 py-1.5 text-sm ${
              isActive
                ? "border-blue-600 bg-slate-800 font-semibold text-slate-100"
                : "border-transparent text-slate-300 hover:border-slate-600 hover:bg-slate-800 hover:text-slate-100"
            }`
          }
        >
          + Import existing setup
        </NavLink>
      </div>
    </nav>
  );
}
