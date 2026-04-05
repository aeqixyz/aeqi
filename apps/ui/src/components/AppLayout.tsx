import { useState, useEffect, useCallback } from "react";
import { useNavigate, useLocation, useSearchParams, Outlet } from "react-router-dom";
import AgentTree from "./Sidebar";
import ContextDrawer from "./ContextDrawer";
import CommandPalette from "./CommandPalette";
import AgentSessionView from "./AgentSessionView";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import ProfileMenu from "./ProfileMenu";
import { useDaemonStore } from "@/store/daemon";
import { useDaemonSocket } from "@/hooks/useDaemonSocket";

export default function AppLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const [params] = useSearchParams();
  const [searching, setSearching] = useState(false);

  const agentId = params.get("agent");
  const sessionId = params.get("session");
  const path = location.pathname;

  const fetchAll = useDaemonStore((s) => s.fetchAll);
  useEffect(() => {
    fetchAll();
    const i = setInterval(fetchAll, 30000);
    return () => clearInterval(i);
  }, [fetchAll]);
  useDaemonSocket();

  const openSearch = useCallback(() => setSearching(true), []);
  const closeSearch = useCallback(() => setSearching(false), []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        if (searching) closeSearch();
        else openSearch();
      }
      if (e.key === "Escape" && searching) {
        closeSearch();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [searching, openSearch, closeSearch]);

  const isActive = (p: string) => {
    if (p === "/") return path === "/" && !agentId;
    return path.startsWith(p) && !agentId;
  };

  return (
    <>
      <div className="shell">
        {/* Left sidebar */}
        <div className="left-sidebar">
          <WorkspaceSwitcher />
          <nav className="sidebar-nav sidebar-nav-no-border">
            <a className={`sidebar-nav-item ${isActive("/events") ? "active" : ""}`} href="/events" onClick={(e) => { e.preventDefault(); navigate("/events"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="2" width="10" height="10" rx="1.5" /><path d="M2 8.5h3l1 1.5h2l1-1.5h3" /></svg>
              <span className="sidebar-nav-label">Events</span>
              <span className="sidebar-nav-action" onClick={(e) => { e.preventDefault(); e.stopPropagation(); navigate("/events?create=1"); }} title="New event">
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M6 2.5v7M2.5 6h7" /></svg>
              </span>
            </a>
            <a className={`sidebar-nav-item ${isActive("/quests") ? "active" : ""}`} href="/quests" onClick={(e) => { e.preventDefault(); navigate("/quests"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3"><path d="M4 3h8M4 7h8M4 11h6M2 3v0M2 7v0M2 11v0" strokeLinecap="round" /></svg>
              <span className="sidebar-nav-label">Quests</span>
              <span className="sidebar-nav-action" onClick={(e) => { e.preventDefault(); e.stopPropagation(); navigate("/quests?create=1"); }} title="New quest">
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M6 2.5v7M2.5 6h7" /></svg>
              </span>
            </a>
            <a className={`sidebar-nav-item ${isActive("/insights") ? "active" : ""}`} href="/insights" onClick={(e) => { e.preventDefault(); navigate("/insights"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3"><path d="M7 2v2M7 10v2M2 7h2M10 7h2M3.8 3.8l1.4 1.4M8.8 8.8l1.4 1.4M10.2 3.8l-1.4 1.4M5.2 8.8l-1.4 1.4" strokeLinecap="round" /></svg>
              <span className="sidebar-nav-label">Insights</span>
              <span className="sidebar-nav-action" onClick={(e) => { e.preventDefault(); e.stopPropagation(); navigate("/insights?create=1"); }} title="New insight">
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M6 2.5v7M2.5 6h7" /></svg>
              </span>
            </a>
            <div className="sidebar-nav-divider" />
            <a className={`sidebar-nav-item ${isActive("/agents") ? "active" : ""}`} href="/agents" onClick={(e) => { e.preventDefault(); navigate("/agents"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3"><circle cx="7" cy="5" r="2.5" /><path d="M3 12.5c0-2.2 1.8-4 4-4s4 1.8 4 4" /></svg>
              <span className="sidebar-nav-label">Agents</span>
              <span className="sidebar-nav-action" onClick={(e) => { e.preventDefault(); e.stopPropagation(); navigate("/agents?create=1"); }} title="New agent">
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M6 2.5v7M2.5 6h7" /></svg>
              </span>
            </a>
          </nav>
          <div className="left-sidebar-body">
            <AgentTree />
          </div>
          <nav className="sidebar-nav sidebar-nav-bottom">
            <a className={`sidebar-nav-item ${isActive("/company") ? "active" : ""}`} href="/company" onClick={(e) => { e.preventDefault(); navigate("/company"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="4" width="10" height="8" rx="1" /><path d="M5 4V3a2 2 0 014 0v1" /></svg>
              <span className="sidebar-nav-label">Company</span>
            </a>
            <a className={`sidebar-nav-item ${isActive("/drive") ? "active" : ""}`} href="/drive" onClick={(e) => { e.preventDefault(); navigate("/drive"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><path d="M2 4.5h10M2 4.5v6a1 1 0 001 1h8a1 1 0 001-1v-6M5 2.5h4a1 1 0 011 1v1H4v-1a1 1 0 011-1z" /></svg>
              <span className="sidebar-nav-label">Drive</span>
            </a>
            <a className={`sidebar-nav-item ${isActive("/apps") ? "active" : ""}`} href="/apps" onClick={(e) => { e.preventDefault(); navigate("/apps"); }}>
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="2" width="4" height="4" rx="0.5" /><rect x="8" y="2" width="4" height="4" rx="0.5" /><rect x="2" y="8" width="4" height="4" rx="0.5" /><rect x="8" y="8" width="4" height="4" rx="0.5" /></svg>
              <span className="sidebar-nav-label">Apps</span>
            </a>
          </nav>
          <ProfileMenu />
        </div>

        {/* Main content */}
        <div className="content-area">
          {agentId ? (
            <AgentSessionView agentId={agentId} sessionId={sessionId} />
          ) : (
            <div className="content-scroll">
              <Outlet />
            </div>
          )}
        </div>

        {/* Right context drawer */}
        <ContextDrawer agentId={agentId} sessionId={sessionId} />
      </div>
      <CommandPalette open={searching} onClose={closeSearch} />
    </>
  );
}
