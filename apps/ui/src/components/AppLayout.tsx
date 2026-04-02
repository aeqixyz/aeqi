import { useState, useEffect, useCallback } from "react";
import { Outlet, NavLink } from "react-router-dom";
import ProjectRail from "./ProjectRail";
import AgentNav from "./Sidebar";
import CommandPalette from "./CommandPalette";

const NAV_ITEMS = [
  { to: "/", label: "home", end: true },
  { to: "/inbox", label: "inbox" },
  { to: "/issues", label: "issues" },
  { to: "/automations", label: "automations" },
  { to: "/knowledge", label: "knowledge" },
  { to: "/finance", label: "finance" },
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
            <span
              className="floating-nav-btn"
              onClick={() => setPaletteOpen(true)}
              title="Search (⌘K)"
            >
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="7" cy="7" r="4.5" />
                <path d="M10.5 10.5L14 14" />
              </svg>
            </span>
            <div className="floating-nav-items">
              {NAV_ITEMS.map((item) => (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.end}
                  className={({ isActive }) =>
                    `floating-nav-item${isActive ? " active" : ""}`
                  }
                >
                  {item.label}
                </NavLink>
              ))}
            </div>
            <span className="floating-nav-btn" title="New">
              <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
                <path d="M8 3v10M3 8h10" />
              </svg>
            </span>
          </div>

          <div className="content-panel">
            <Outlet />
          </div>
        </div>
      </div>
      <CommandPalette open={paletteOpen} onClose={closePalette} />
    </div>
  );
}
