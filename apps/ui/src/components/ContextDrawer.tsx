import { useEffect, useCallback, useState, useRef } from "react";
import { useUIStore } from "@/store/ui";
import { useChatStore } from "@/store/chat";
import { useDaemonStore } from "@/store/daemon";
import BlockAvatar from "./BlockAvatar";
import ContextView from "./drawer/ContextView";
import ActivityView from "./drawer/ActivityView";
import QuickActions from "./drawer/QuickActions";

interface Props {
  agentId: string | null;
  sessionId: string | null;
}

export default function ContextDrawer({ agentId, sessionId }: Props) {
  const drawerOpen = useUIStore((s) => s.drawerOpen);
  const toggleDrawer = useUIStore((s) => s.toggleDrawer);
  const mode = useUIStore((s) => s.drawerMode);
  const setMode = useUIStore((s) => s.setDrawerMode);

  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const agents = useDaemonStore((s) => s.agents);
  const wsConnected = useDaemonStore((s) => s.wsConnected);

  const agentInfo = agents.find(
    (a) => a.id === agentId || a.name === agentId || a.name === selectedAgent?.name,
  );
  const displayName = agentInfo?.display_name || agentInfo?.name || agentId || "";

  // Cmd+. to toggle
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === ".") {
        e.preventDefault();
        toggleDrawer();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [toggleDrawer]);

  // Resize
  const [width, setWidth] = useState(() =>
    parseInt(localStorage.getItem("aeqi_drawer_width") || "320"),
  );
  const resizing = useRef(false);

  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizing.current = true;
    document.body.style.userSelect = "none";
    document.body.style.cursor = "col-resize";
    const startX = e.clientX;
    const startW = width;

    const onMove = (ev: MouseEvent) => {
      const delta = startX - ev.clientX;
      const next = Math.max(280, Math.min(480, startW + delta));
      setWidth(next);
    };
    const onUp = () => {
      resizing.current = false;
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      localStorage.setItem("aeqi_drawer_width", String(width));
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }, [width]);

  if (!agentId) return null;

  // Collapsed state — show toggle button
  if (!drawerOpen) {
    return (
      <button
        className="drawer-toggle-collapsed"
        onClick={toggleDrawer}
        title="Open panel (⌘.)"
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
          <path d="M9 3L5 7l4 4" />
        </svg>
      </button>
    );
  }

  return (
    <aside className="context-drawer" style={{ width }}>
      {/* Resize handle */}
      <div className="context-drawer-resize" onMouseDown={onResizeStart} />

      {/* Header */}
      <div className="drawer-header">
        <div className="drawer-header-agent">
          <BlockAvatar name={displayName} size={22} />
          <div className="drawer-header-text">
            <span className="drawer-header-name">{displayName}</span>
            {agentInfo?.model && (
              <span className="drawer-header-model">{agentInfo.model}</span>
            )}
          </div>
          <span className={`drawer-header-dot ${wsConnected ? "live" : ""}`} />
        </div>
        <button className="drawer-collapse-btn" onClick={toggleDrawer} title="Close panel (⌘.)">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
            <path d="M5 3l4 4-4 4" />
          </svg>
        </button>
      </div>

      {/* Session banner */}
      {sessionId && (
        <div className="drawer-session-banner">
          <span className="drawer-session-dot live" />
          <span className="drawer-session-label">Active session</span>
        </div>
      )}

      {/* Segment control */}
      <div className="drawer-segment-control">
        <button
          className={`drawer-segment-btn ${mode === "context" ? "active" : ""}`}
          onClick={() => setMode("context")}
        >
          Context
        </button>
        <button
          className={`drawer-segment-btn ${mode === "activity" ? "active" : ""}`}
          onClick={() => setMode("activity")}
        >
          Activity
        </button>
      </div>

      {/* Body */}
      <div className="drawer-body">
        {mode === "context" ? (
          <ContextView agentName={displayName} />
        ) : (
          <ActivityView agentName={displayName} agentId={agentId} />
        )}
      </div>

      {/* Quick actions */}
      <QuickActions agentId={agentId} agentName={displayName} />
    </aside>
  );
}
