import { useState, useEffect, useCallback } from "react";
import { Outlet, NavLink } from "react-router-dom";
import ProjectRail from "./ProjectRail";
import AgentNav from "./Sidebar";
import CommandPalette from "./CommandPalette";

function NavIcon({ d, size = 16 }: { d: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
      <path d={d} />
    </svg>
  );
}

const NAV_ITEMS = [
  {
    to: "/",
    label: "Home",
    icon: "M8 1.5L1.5 7v7h4.5v-4h4v4h4.5V7L8 1.5z",
    end: true,
  },
  {
    to: "/inbox",
    label: "Inbox",
    icon: "M2 4l6 4 6-4M2 4v8h12V4",
  },
  {
    to: "/issues",
    label: "Issues",
    icon: "M3 3h10v10H3zM6 6h4M6 8h4M6 10h2",
  },
  {
    to: "/automations",
    label: "Automations",
    icon: "M8 2v3M4.5 4.5l2 2M11.5 4.5l-2 2M2 8h3M11 8h3M4.5 11.5l2-2M11.5 11.5l-2-2M8 11v3",
  },
  {
    to: "/knowledge",
    label: "Knowledge",
    icon: "M2 2h5l1 1.5L9 2h5v11H9l-1 1-1-1H2z",
  },
  {
    to: "/finance",
    label: "Finance",
    icon: "M2 13V5h3v8M6.5 13V3h3v10M11 13V7h3v6",
  },
];

export default function AppLayout() {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const closePalette = useCallback(() => setPaletteOpen(false), []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((p) => !p);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="shell">
      <ProjectRail />
      <AgentNav />
      <div className="content-area">
        <div className="content-scroll">
          <div className="floating-nav">
            <div className="floating-nav-items">
              {NAV_ITEMS.map((item) => (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.end}
                  className={({ isActive }) =>
                    `floating-nav-item${isActive ? " active" : ""}`
                  }
                  title={item.label}
                >
                  <NavIcon d={item.icon} />
                </NavLink>
              ))}
            </div>
            <div className="floating-nav-actions">
              <span
                className="floating-nav-kbd"
                onClick={() => setPaletteOpen(true)}
              >
                ⌘K
              </span>
            </div>
          </div>

          <Outlet />
        </div>
      </div>
      <CommandPalette open={paletteOpen} onClose={closePalette} />
    </div>
  );
}
