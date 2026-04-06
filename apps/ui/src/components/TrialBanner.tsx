import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import BrandMark from "./BrandMark";

export default function TrialBanner() {
  const navigate = useNavigate();
  const isTrialing = useAuthStore((s) => s.isTrialing);
  const trialDaysLeft = useAuthStore((s) => s.trialDaysLeft);
  const [dismissed, setDismissed] = useState(false);

  if (!isTrialing() || dismissed) return null;

  const days = trialDaysLeft();
  const expired = days === 0;

  return (
    <div className={`trial-banner${expired ? " expired" : ""}`}>
      <span className="trial-banner-text">
        {expired
          ? <>Your trial has expired. Upgrade to continue using <BrandMark size={13} />.</>
          : `${days} day${days !== 1 ? "s" : ""} left on your free trial`}
      </span>
      <div className="trial-banner-actions">
        <button className="trial-banner-cta" onClick={() => navigate("/billing")}>
          {expired ? "Choose a plan" : "Upgrade"}
        </button>
        {!expired && (
          <button className="trial-banner-dismiss" onClick={() => setDismissed(true)} title="Dismiss">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
              <path d="M4 4l6 6M10 4l-6 6" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}
