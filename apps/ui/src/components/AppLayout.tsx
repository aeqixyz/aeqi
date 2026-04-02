import { Fragment, useState, useEffect, useCallback } from "react";
import { Outlet, useLocation } from "react-router-dom";
import ProjectRail from "./ProjectRail";
import SecondaryNav from "./Sidebar";
import CommandPalette from "./CommandPalette";

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

  // Derive breadcrumb from path
  const segments = location.pathname.split("/").filter(Boolean);

  return (
    <div className="shell">
      <ProjectRail />
      <SecondaryNav />
      <div className="content-area">
        <div className="topbar">
          <div className="topbar-breadcrumb">
            {segments.length === 0 ? (
              <span>Chat</span>
            ) : (
              segments.map((s, i) => (
                <Fragment key={i}>
                  {i > 0 && <span className="topbar-breadcrumb-sep">/</span>}
                  <span>{decodeURIComponent(s).charAt(0).toUpperCase() + decodeURIComponent(s).slice(1)}</span>
                </Fragment>
              ))
            )}
          </div>
          <div className="topbar-actions">
            <span
              className="topbar-kbd"
              onClick={() => setPaletteOpen(true)}
            >
              ⌘K
            </span>
          </div>
        </div>
        <div className="content-scroll">
          <Outlet />
        </div>
      </div>
      <CommandPalette open={paletteOpen} onClose={closePalette} />
    </div>
  );
}
