import { useState, useEffect, useCallback } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import AgentTree from "./Sidebar";
import ContextPanel from "./ContextPanel";
import BlockAvatar from "./BlockAvatar";
import CommandPalette from "./CommandPalette";
import AgentSessionView from "./AgentSessionView";
import DashboardHome from "./DashboardHome";
import { useDaemonStore } from "@/store/daemon";
import { useDaemonSocket } from "@/hooks/useDaemonSocket";

export default function AppLayout() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [searching, setSearching] = useState(false);

  const agentId = params.get("agent");
  const sessionId = params.get("session");

  const fetchAll = useDaemonStore((s) => s.fetchAll);
  useEffect(() => { fetchAll(); const i = setInterval(fetchAll, 30000); return () => clearInterval(i); }, [fetchAll]);
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

  return (
    <>
      <div className="shell">
        {/* Left sidebar: Agent tree */}
        <div className="left-sidebar">
          <div className="sidebar-profile sidebar-profile-top">
            <a href="/" className="sidebar-brand-mark">æ</a>
            <div className="sidebar-profile-info">
              <span className="sidebar-profile-name">aeqi.ai</span>
              <span className="sidebar-profile-plan">hosted</span>
            </div>
            <span className="sidebar-profile-settings" onClick={openSearch} title="Search (Cmd+K)">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="7" cy="7" r="4.5" />
                <path d="M10.5 10.5L14 14" />
              </svg>
            </span>
          </div>
          <div className="left-sidebar-body">
            <AgentTree />
          </div>
          <div className="sidebar-profile">
            <BlockAvatar name={userName} size={32} />
            <div className="sidebar-profile-info">
              <span className="sidebar-profile-name">{userName}</span>
              <span className="sidebar-profile-plan">free plan</span>
            </div>
            <span className="sidebar-profile-settings" onClick={() => navigate("/settings")} title="Settings">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="8" cy="8" r="2.5" />
                <path d="M13.5 8a5.5 5.5 0 01-.4 1.6l1.1 1.3-1.1 1.1-1.3-1.1A5.5 5.5 0 018 13.5a5.5 5.5 0 01-3.8-2.6L3 12l-1.1-1.1 1.1-1.3A5.5 5.5 0 012.5 8a5.5 5.5 0 01.5-1.6L1.9 5.1 3 4l1.3 1.1A5.5 5.5 0 018 2.5a5.5 5.5 0 013.8 2.6L13 4l1.1 1.1-1.1 1.3A5.5 5.5 0 0113.5 8z" />
              </svg>
            </span>
          </div>
        </div>

        {/* Main content: Session view or Dashboard */}
        <div className="content-area">
          {agentId ? (
            <AgentSessionView agentId={agentId} sessionId={sessionId} />
          ) : (
            <div className="content-scroll">
              <DashboardHome />
            </div>
          )}
        </div>

        {/* Right context panel: visible when agent selected */}
        {agentId && <ContextPanel />}
      </div>
      <CommandPalette open={searching} onClose={closeSearch} />
    </>
  );
}
