import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import { useDaemonStore } from "@/store/daemon";
import BlockAvatar from "./BlockAvatar";

function formatUsage(usd: number): string {
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  if (usd >= 0.01) return `$${usd.toFixed(2)}`;
  if (usd > 0) return `<$0.01`;
  return "$0.00";
}

export default function ProfileMenu() {
  const navigate = useNavigate();
  const logout = useAuthStore((s) => s.logout);
  const authMode = useAuthStore((s) => s.authMode);
  const user = useAuthStore((s) => s.user);
  const fetchMe = useAuthStore((s) => s.fetchMe);
  const cost = useDaemonStore((s) => s.cost);
  const fetchCost = useDaemonStore((s) => s.fetchCost);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (authMode === "accounts" && !user) fetchMe();
  }, [authMode, user, fetchMe]);

  useEffect(() => {
    if (open && !cost) fetchCost();
  }, [open, cost, fetchCost]);

  const userName = user?.name || (authMode === "none" ? "Self-hosted" : "Operator");
  const userEmail = user?.email || "";

  const spentToday = cost?.spent_today_usd ?? cost?.cost_today_usd ?? 0;
  const budget = cost?.daily_budget_usd ?? 0;
  const remaining = budget > 0 ? Math.max(0, budget - spentToday) : 0;

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
            <span className="pm-credits-label">Today's usage</span>
            <span className="pm-credits-value">
              {formatUsage(spentToday)}
              {budget > 0 && (
                <span style={{ color: "rgba(0,0,0,0.3)", fontWeight: 400 }}>
                  {" "}/ {formatUsage(budget)}
                </span>
              )}
            </span>
          </div>
          <button className="pm-item pm-item-accent" onClick={() => { setOpen(false); navigate("/settings?tab=usage"); }}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><path d="M2 10V4l3 2 2-4 2 4 3-2v6" /></svg>
            View usage
          </button>
          <button className="pm-item" onClick={() => { setOpen(false); navigate("/billing"); }}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="3.5" width="10" height="7" rx="1" /><path d="M2 6h10" /></svg>
            Billing
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
          {authMode !== "none" && (
            <>
              <div className="pm-divider" />
              <button className="pm-item pm-item-muted" onClick={handleLogout}>
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><path d="M5 2H3.5a1 1 0 00-1 1v8a1 1 0 001 1H5M8 10l3-3-3-3M11 7H5" /></svg>
                Sign out
              </button>
            </>
          )}
        </div>
      )}

      <div className="pm-trigger">
        <div className="pm-trigger-profile" onClick={() => { setOpen(false); navigate("/settings"); }}>
          {user?.avatar_url ? (
            <img src={user.avatar_url} alt="" style={{ width: 22, height: 22, borderRadius: 4, flexShrink: 0 }} />
          ) : (
            <BlockAvatar name={userName} size={22} />
          )}
          <div className="pm-trigger-text">
            <span className="pm-trigger-name">{userName}</span>
            <span className="pm-trigger-plan">
              {authMode === "none"
                ? "local"
                : user?.subscription_plan
                  ? `${user.subscription_plan} plan`
                  : user?.subscription_status === "trialing"
                    ? "free trial"
                    : "free plan"}
            </span>
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
