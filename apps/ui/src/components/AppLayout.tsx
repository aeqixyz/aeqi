import { useState, useEffect, useCallback } from "react";
import { useNavigate, useLocation, useSearchParams, Outlet } from "react-router-dom";
import AgentTree from "./Sidebar";
import ContextDrawer from "./ContextDrawer";
import BlockAvatar from "./BlockAvatar";
import CommandPalette from "./CommandPalette";
import AgentSessionView from "./AgentSessionView";
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

  const userName = localStorage.getItem("aeqi_user_name") || "Operator";

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
          <div className="sidebar-profile sidebar-profile-top">
            <a href="/" className="sidebar-brand-mark">
              æ
            </a>
            <div className="sidebar-profile-info">
              <span className="sidebar-profile-name">aeqi.ai</span>
              <span className="sidebar-profile-plan">hosted</span>
            </div>
            <span
              className="sidebar-profile-settings"
              onClick={openSearch}
              title="Search (Cmd+K)"
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <circle cx="7" cy="7" r="4.5" />
                <path d="M10.5 10.5L14 14" />
              </svg>
            </span>
          </div>
          <nav className="sidebar-nav">
            <a
              className={`sidebar-nav-item ${isActive("/agents") ? "active" : ""}`}
              href="/agents"
              onClick={(e) => {
                e.preventDefault();
                navigate("/agents");
              }}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 14 14"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
              >
                <circle cx="7" cy="5" r="2.5" />
                <path d="M3 12.5c0-2.2 1.8-4 4-4s4 1.8 4 4" />
              </svg>
              Agents
            </a>
            <a
              className={`sidebar-nav-item ${isActive("/events") ? "active" : ""}`}
              href="/events"
              onClick={(e) => {
                e.preventDefault();
                navigate("/events");
              }}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 14 14"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
              >
                <path d="M2 4l5 3.5L12 4M2 4v6.5h10V4" />
              </svg>
              Events
            </a>
            <a
              className={`sidebar-nav-item ${isActive("/quests") ? "active" : ""}`}
              href="/quests"
              onClick={(e) => {
                e.preventDefault();
                navigate("/quests");
              }}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 14 14"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
              >
                <path
                  d="M4 3h8M4 7h8M4 11h6M2 3v0M2 7v0M2 11v0"
                  strokeLinecap="round"
                />
              </svg>
              Quests
            </a>
            <a
              className={`sidebar-nav-item ${isActive("/insights") ? "active" : ""}`}
              href="/insights"
              onClick={(e) => {
                e.preventDefault();
                navigate("/insights");
              }}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 14 14"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
              >
                <path
                  d="M7 2v2M7 10v2M2 7h2M10 7h2M3.8 3.8l1.4 1.4M8.8 8.8l1.4 1.4M10.2 3.8l-1.4 1.4M5.2 8.8l-1.4 1.4"
                  strokeLinecap="round"
                />
              </svg>
              Insights
            </a>
          </nav>
          <div className="left-sidebar-body">
            <AgentTree />
          </div>
          <nav className="sidebar-nav sidebar-nav-bottom">
            <a
              className={`sidebar-nav-item ${isActive("/company") ? "active" : ""}`}
              href="/company"
              onClick={(e) => {
                e.preventDefault();
                navigate("/company");
              }}
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round">
                <rect x="2" y="4" width="10" height="8" rx="1" />
                <path d="M5 4V3a2 2 0 014 0v1" />
              </svg>
              Company
            </a>
            <a
              className={`sidebar-nav-item ${isActive("/drive") ? "active" : ""}`}
              href="/drive"
              onClick={(e) => {
                e.preventDefault();
                navigate("/drive");
              }}
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round">
                <path d="M2 4.5h10M2 4.5v6a1 1 0 001 1h8a1 1 0 001-1v-6M5 2.5h4a1 1 0 011 1v1H4v-1a1 1 0 011-1z" />
              </svg>
              Drive
            </a>
            <a
              className={`sidebar-nav-item ${isActive("/apps") ? "active" : ""}`}
              href="/apps"
              onClick={(e) => {
                e.preventDefault();
                navigate("/apps");
              }}
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round">
                <rect x="2" y="2" width="4" height="4" rx="0.5" />
                <rect x="8" y="2" width="4" height="4" rx="0.5" />
                <rect x="2" y="8" width="4" height="4" rx="0.5" />
                <rect x="8" y="8" width="4" height="4" rx="0.5" />
              </svg>
              Apps
            </a>
          </nav>
          <div className="sidebar-profile">
            <BlockAvatar name={userName} size={22} />
            <div className="sidebar-profile-info">
              <span className="sidebar-profile-name">{userName}</span>
              <span className="sidebar-profile-plan">free plan</span>
            </div>
            <span
              className="sidebar-profile-settings"
              onClick={() => navigate("/settings")}
              title="Settings"
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <circle cx="8" cy="8" r="2.5" />
                <path d="M13.5 8a5.5 5.5 0 01-.4 1.6l1.1 1.3-1.1 1.1-1.3-1.1A5.5 5.5 0 018 13.5a5.5 5.5 0 01-3.8-2.6L3 12l-1.1-1.1 1.1-1.3A5.5 5.5 0 012.5 8a5.5 5.5 0 01.5-1.6L1.9 5.1 3 4l1.3 1.1A5.5 5.5 0 018 2.5a5.5 5.5 0 013.8 2.6L13 4l1.1 1.1-1.1 1.3A5.5 5.5 0 0113.5 8z" />
              </svg>
            </span>
          </div>
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
