import { Link, useLocation } from "react-router-dom";
import { useEffect, useState } from "react";
import { useChatStore } from "@/store/chat";
import { useUIStore } from "@/store/ui";
import { api } from "@/lib/api";

export default function Sidebar({ onCommandPalette }: { onCommandPalette: () => void }) {
  const { pathname } = useLocation();
  const channel = useChatStore((s) => s.channel);
  const setChannel = useChatStore((s) => s.setChannel);
  const collapsed = useUIStore((s) => s.sidebarCollapsed);
  const toggleSidebar = useUIStore((s) => s.toggleSidebar);
  const [departments, setDepartments] = useState<any[]>([]);

  useEffect(() => {
    if (!channel) return;
    api.getProjects().then((d) => {
      const proj = (d.projects || []).find((p: any) => p.name === channel);
      setDepartments(proj?.departments || []);
    }).catch(() => {});
  }, [channel]);

  const isChatHome = pathname === "/";

  if (collapsed) {
    return (
      <aside className="sidebar sidebar-collapsed">
        <div className="sidebar-top">
          <button className="sidebar-toggle" onClick={toggleSidebar} title="Expand (Cmd+B)">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5">
              <path d="M5 3l4 4-4 4" />
            </svg>
          </button>
        </div>
        <nav className="sidebar-collapsed-nav">
          {COLLAPSED_ITEMS.map((item) => (
            <Link
              key={item.href}
              to={item.href}
              className={`sidebar-collapsed-item ${
                item.href === "/" ? (isChatHome ? "sidebar-collapsed-item-active" : "") :
                pathname.startsWith(item.href) ? "sidebar-collapsed-item-active" : ""
              }`}
              title={item.title}
            >
              {item.icon}
            </Link>
          ))}
        </nav>
      </aside>
    );
  }

  return (
    <aside className="sidebar">
      {/* Header */}
      <div className="sidebar-top">
        <button className="sidebar-toggle" onClick={toggleSidebar} title="Collapse (Cmd+B)">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M9 3l-4 4 4 4" />
          </svg>
        </button>
        <span className="sidebar-context-label">
          {channel || "sigil"}
        </span>
        <div className="sidebar-spacer" />
        <button className="sidebar-cmd" onClick={onCommandPalette} title="Cmd+K">
          <kbd>K</kbd>
        </button>
      </div>

      {/* Department channels (when a project is selected) */}
      {channel && departments.length > 0 && (
        <div className="sidebar-section">
          <div className="sidebar-section-header">Channels</div>
          <div
            className={`channel-item ${isChatHome && !channel.includes("/") ? "channel-item-active" : ""}`}
            onClick={() => setChannel(channel.split("/")[0])}
          >
            <span className="channel-hash">#</span>
            <span className="channel-name">general</span>
          </div>
          {departments.map((d: any) => {
            const deptChannel = `${channel.split("/")[0]}/${d.name}`;
            return (
              <div
                key={deptChannel}
                className={`channel-item ${useChatStore.getState().channel === deptChannel ? "channel-item-active" : ""}`}
                onClick={() => setChannel(deptChannel)}
              >
                <span className="channel-hash">#</span>
                <span className="channel-name">{d.name}</span>
              </div>
            );
          })}
        </div>
      )}

      {/* Navigation */}
      <div className="sidebar-nav">
        {NAV_ITEMS.map((item) => (
          <Link
            key={item.href}
            to={item.href}
            className={`nav-item ${pathname === item.href || (item.href !== "/" && pathname.startsWith(item.href)) ? "nav-item-active" : ""}`}
          >
            <NavIcon name={item.icon} />
            <span>{item.label}</span>
          </Link>
        ))}
      </div>

      {/* Footer */}
      <div className="sidebar-bottom">
        <span className="sidebar-version">v0.3.0</span>
      </div>
    </aside>
  );
}

const COLLAPSED_ITEMS = [
  { href: "/", title: "Chat", icon: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 2h10a1 1 0 011 1v6a1 1 0 01-1 1H5l-3 3V3a1 1 0 011-1z"/></svg> },
  { href: "/dashboard", title: "Dashboard", icon: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="8" y="1" width="5" height="5" rx="1"/><rect x="1" y="8" width="5" height="5" rx="1"/><rect x="8" y="8" width="5" height="5" rx="1"/></svg> },
  { href: "/tasks", title: "Tasks", icon: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 3h10M2 7h10M2 11h6"/></svg> },
  { href: "/agents", title: "Agents", icon: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="2" y="1" width="10" height="8" rx="2"/><circle cx="5" cy="5" r="1"/><circle cx="9" cy="5" r="1"/><path d="M4 12h6"/><path d="M7 9v3"/></svg> },
  { href: "/knowledge", title: "Knowledge", icon: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 2h4l1 1h5v9H2V2z"/></svg> },
  { href: "/settings", title: "Settings", icon: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="2"/><path d="M7 1v2M7 11v2M1 7h2M11 7h2M2.8 2.8l1.4 1.4M9.8 9.8l1.4 1.4M2.8 11.2l1.4-1.4M9.8 4.2l1.4-1.4"/></svg> },
];

const NAV_ITEMS = [
  { href: "/", label: "Chat", icon: "chat" },
  { href: "/dashboard", label: "Dashboard", icon: "grid" },
  { href: "/projects", label: "Projects", icon: "box" },
  { href: "/agents", label: "Agents", icon: "bot" },
  { href: "/tasks", label: "Tasks", icon: "list" },
  { href: "/missions", label: "Missions", icon: "target" },
  { href: "/knowledge", label: "Knowledge", icon: "book" },
  { href: "/operations", label: "Operations", icon: "clock" },
  { href: "/cost", label: "Cost", icon: "dollar" },
  { href: "/settings", label: "Settings", icon: "gear" },
];

function NavIcon({ name }: { name: string }) {
  const icons: Record<string, React.ReactNode> = {
    chat: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 2h10a1 1 0 011 1v6a1 1 0 01-1 1H5l-3 3V3a1 1 0 011-1z"/></svg>,
    grid: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="8" y="1" width="5" height="5" rx="1"/><rect x="1" y="8" width="5" height="5" rx="1"/><rect x="8" y="8" width="5" height="5" rx="1"/></svg>,
    box: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 4l5-2 5 2v6l-5 2-5-2V4z"/><path d="M2 4l5 2 5-2"/><path d="M7 6v6"/></svg>,
    bot: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="2" y="1" width="10" height="8" rx="2"/><circle cx="5" cy="5" r="1"/><circle cx="9" cy="5" r="1"/><path d="M4 12h6"/><path d="M7 9v3"/></svg>,
    list: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 3h10M2 7h10M2 11h6"/></svg>,
    target: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="5.5"/><circle cx="7" cy="7" r="3"/><circle cx="7" cy="7" r="0.5" fill="currentColor"/></svg>,
    book: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M2 2h4l1 1h5v9H2V2z"/></svg>,
    clock: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="5.5"/><path d="M7 4v3l2 1"/></svg>,
    dollar: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="5.5"/><path d="M7 4v6M5 6h4M5 8h4"/></svg>,
    gear: <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="7" cy="7" r="2"/><path d="M7 1v2M7 11v2M1 7h2M11 7h2M2.8 2.8l1.4 1.4M9.8 9.8l1.4 1.4M2.8 11.2l1.4-1.4M9.8 4.2l1.4-1.4"/></svg>,
  };
  return <span className="nav-icon">{icons[name]}</span>;
}
