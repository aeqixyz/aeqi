import { useState, useEffect, useCallback } from "react";
import { Outlet, useLocation, useNavigate, NavLink } from "react-router-dom";
import ProjectRail from "./ProjectRail";
import AgentNav from "./Sidebar";
import CommandPalette from "./CommandPalette";

const NAV_ITEMS = [
  { to: "/", label: "Chat", end: true },
  { to: "/tasks", label: "Tasks" },
  { to: "/memory", label: "Memory" },
  { to: "/skills", label: "Skills" },
  { to: "/triggers", label: "Triggers" },
  { to: "/audit", label: "Audit" },
  { to: "/cost", label: "Cost" },
];

export default function AppLayout() {
  const location = useLocation();
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
          {/* Floating nav bar */}
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
                >
                  {item.label}
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

          {/* Page content */}
          <Outlet />
        </div>
      </div>
      <CommandPalette open={paletteOpen} onClose={closePalette} />
    </div>
  );
}
