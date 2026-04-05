import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import BlockAvatar from "./BlockAvatar";

export default function ProfileMenu() {
  const navigate = useNavigate();
  const logout = useAuthStore((s) => s.logout);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const userName = localStorage.getItem("aeqi_user_name") || "Operator";
  const userEmail = localStorage.getItem("aeqi_user_email") || "";

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleLogout = () => {
    logout();
    navigate("/login");
  };

  return (
    <div className="pm-container" ref={ref}>
      {open && (
        <div className="pm-dropup">
          <div className="pm-header">
            <span className="pm-header-name">{userName}</span>
            {userEmail && <span className="pm-header-email">{userEmail}</span>}
          </div>
          <div className="pm-divider" />
          <div className="pm-credits">
            <span className="pm-credits-label">Credits</span>
            <span className="pm-credits-value">12.32 credits</span>
          </div>
          <button className="pm-item pm-item-accent" onClick={() => { setOpen(false); navigate("/settings?tab=billing"); }}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><path d="M7 2l1.5 3 3.5.5-2.5 2.5.5 3.5L7 10l-3 1.5.5-3.5L2 5.5 5.5 5z" /></svg>
            Upgrade Plan
          </button>
          <button className="pm-item" onClick={() => { setOpen(false); navigate("/settings?tab=billing"); }}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="3.5" width="10" height="7" rx="1" /><path d="M2 6h10" /></svg>
            Top Up Credits
          </button>
          <div className="pm-divider" />
          <button className="pm-item" onClick={() => { setOpen(false); navigate("/settings"); }}>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><circle cx="8" cy="8" r="2.5" /><path d="M13.5 8a5.5 5.5 0 01-.4 1.6l1.1 1.3-1.1 1.1-1.3-1.1A5.5 5.5 0 018 13.5a5.5 5.5 0 01-3.8-2.6L3 12l-1.1-1.1 1.1-1.3A5.5 5.5 0 012.5 8a5.5 5.5 0 01.5-1.6L1.9 5.1 3 4l1.3 1.1A5.5 5.5 0 018 2.5a5.5 5.5 0 013.8 2.6L13 4l1.1 1.1-1.1 1.3A5.5 5.5 0 0113.5 8z" /></svg>
            Settings
          </button>
          <button className="pm-item" onClick={() => { setOpen(false); }}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><circle cx="7" cy="7" r="5" /><path d="M7 4v3M7 9v0" /></svg>
            Support
          </button>
          <div className="pm-divider" />
          <button className="pm-item pm-item-muted" onClick={handleLogout}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><path d="M5 2H3.5a1 1 0 00-1 1v8a1 1 0 001 1H5M8 10l3-3-3-3M11 7H5" /></svg>
            Sign out
          </button>
        </div>
      )}

      <div className="pm-trigger">
        <div className="pm-trigger-profile" onClick={() => { setOpen(false); navigate("/settings"); }}>
          <BlockAvatar name={userName} size={22} />
          <div className="pm-trigger-text">
            <span className="pm-trigger-name">{userName}</span>
            <span className="pm-trigger-plan">free plan</span>
          </div>
        </div>
        <button className="ws-chevron-btn" onClick={() => setOpen(!open)} title="User menu">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
            <path d="M4 3l2-1.5L8 3" />
            <path d="M4 9l2 1.5L8 9" />
          </svg>
        </button>
      </div>
    </div>
  );
}
