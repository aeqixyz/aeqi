import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useUIStore } from "@/store/ui";
import BlockAvatar from "@/components/BlockAvatar";
import "@/styles/welcome.css";

// Same SVGs as sidebar
const ICONS: Record<string, React.ReactNode> = {
  agents: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3"><circle cx="7" cy="5" r="2.5" /><path d="M3 12.5c0-2.2 1.8-4 4-4s4 1.8 4 4" /></svg>,
  events: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="2" width="10" height="10" rx="1.5" /><path d="M2 8.5h3l1 1.5h2l1-1.5h3" /></svg>,
  quests: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3"><path d="M4 3h8M4 7h8M4 11h6M2 3v0M2 7v0M2 11v0" strokeLinecap="round" /></svg>,
  insights: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3"><path d="M7 2v2M7 10v2M2 7h2M10 7h2M3.8 3.8l1.4 1.4M8.8 8.8l1.4 1.4M10.2 3.8l-1.4 1.4M5.2 8.8l-1.4 1.4" strokeLinecap="round" /></svg>,
  company: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="4" width="10" height="8" rx="1" /><path d="M5 4V3a2 2 0 014 0v1" /></svg>,
  drive: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><path d="M2 4.5h10M2 4.5v6a1 1 0 001 1h8a1 1 0 001-1v-6M5 2.5h4a1 1 0 011 1v1H4v-1a1 1 0 011-1z" /></svg>,
  apps: <svg width="16" height="16" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><rect x="2" y="2" width="4" height="4" rx="0.5" /><rect x="8" y="2" width="4" height="4" rx="0.5" /><rect x="2" y="8" width="4" height="4" rx="0.5" /><rect x="8" y="8" width="4" height="4" rx="0.5" /></svg>,
};

const ITEMS = [
  { key: "agents", name: "Agents", desc: "Autonomous entities that research, plan, implement, and verify.", route: "/agents" },
  { key: "events", name: "Events", desc: "Real-time activity stream. Decisions, messages, and approvals.", route: "/events" },
  { key: "quests", name: "Quests", desc: "Units of work tracked through your agent pipeline.", route: "/quests" },
  { key: "insights", name: "Insights", desc: "Knowledge your agents accumulate and share across sessions.", route: "/insights" },
  { key: "company", name: "Company", desc: "Team, settings, and configuration for this company.", route: "/company" },
  { key: "drive", name: "Drive", desc: "Files, prompts, agent templates, and artifacts.", route: "/drive" },
  { key: "apps", name: "Apps", desc: "Integrations, MCP tools, and third-party connections.", route: "/apps" },
];

export default function WelcomePage() {
  const navigate = useNavigate();
  const activeCompany = useUIStore((s) => s.activeCompany);
  const setActiveCompany = useUIStore((s) => s.setActiveCompany);

  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState(activeCompany);
  const [tagline, setTagline] = useState(
    () => localStorage.getItem("aeqi_company_tagline") || "The agent runtime.",
  );
  const [editingTagline, setEditingTagline] = useState(false);
  const [taglineDraft, setTaglineDraft] = useState(tagline);

  const saveName = () => {
    if (nameDraft.trim()) setActiveCompany(nameDraft.trim());
    setEditingName(false);
  };
  const saveTagline = () => {
    const val = taglineDraft.trim() || "The agent runtime.";
    setTagline(val);
    localStorage.setItem("aeqi_company_tagline", val);
    setEditingTagline(false);
  };

  const displayName = activeCompany || "aeqi";

  return (
    <div className="welcome">
      <div className="welcome-inner new-ws-animate">
        {/* Identity — same pattern as /new */}
        <div className="welcome-identity">
          <BlockAvatar name={displayName} size={56} />
          <div className="welcome-identity-text">
            {editingName ? (
              <input
                className="new-ws-name-input"
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                onBlur={saveName}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveName();
                  if (e.key === "Escape") { setEditingName(false); setNameDraft(activeCompany); }
                }}
                autoFocus
              />
            ) : (
              <h1
                className="welcome-name"
                onClick={() => { setEditingName(true); setNameDraft(activeCompany); }}
                title="Click to rename"
              >
                {displayName}
              </h1>
            )}
            {editingTagline ? (
              <input
                className="new-ws-tagline-input"
                value={taglineDraft}
                onChange={(e) => setTaglineDraft(e.target.value)}
                onBlur={saveTagline}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveTagline();
                  if (e.key === "Escape") { setEditingTagline(false); setTaglineDraft(tagline); }
                }}
                autoFocus
              />
            ) : (
              <p
                className="welcome-sub"
                onClick={() => { setEditingTagline(true); setTaglineDraft(tagline); }}
                title="Click to edit"
              >
                {tagline}
              </p>
            )}
          </div>
        </div>

        {/* Navigation grid */}
        <div className="welcome-grid">
          {ITEMS.map((item) => (
            <div key={item.key} className="welcome-card" onClick={() => navigate(item.route)}>
              <span className="welcome-card-icon">{ICONS[item.key]}</span>
              <div className="welcome-card-body">
                <h3>{item.name}</h3>
                <p>{item.desc}</p>
              </div>
              <svg className="welcome-card-arrow" width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M5 3l4 4-4 4" /></svg>
            </div>
          ))}
        </div>

        {/* New company */}
        <button className="welcome-new-ws" onClick={() => navigate("/new")}>
          + New company
        </button>

        {/* Footer */}
        <div className="welcome-footer">
          <p>
            By using aeqi.ai you agree to our{" "}
            <a href="/terms" className="welcome-link">Terms of Service</a>
            {" "}and{" "}
            <a href="/privacy" className="welcome-link">Privacy Policy</a>.
          </p>
        </div>
      </div>
    </div>
  );
}
