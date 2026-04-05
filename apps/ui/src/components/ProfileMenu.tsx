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
            Upgrade Plan
          </button>
          <button className="pm-item" onClick={() => { setOpen(false); navigate("/settings?tab=billing"); }}>
            Top Up Credits
          </button>
          <div className="pm-divider" />
          <button className="pm-item" onClick={() => { setOpen(false); navigate("/settings"); }}>
            Settings
          </button>
          <button className="pm-item" onClick={() => { setOpen(false); }}>
            Support
          </button>
          <div className="pm-divider" />
          <button className="pm-item pm-item-danger" onClick={handleLogout}>
            Sign out
          </button>
        </div>
      )}

      <div className="pm-trigger">
        <BlockAvatar name={userName} size={22} />
        <div className="pm-trigger-text" onClick={() => setOpen(!open)}>
          <span className="pm-trigger-name">{userName}</span>
          <span className="pm-trigger-plan">free plan</span>
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
