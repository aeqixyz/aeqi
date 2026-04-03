import { useEffect, useState } from "react";
import Header from "@/components/Header";
import EmptyState from "@/components/EmptyState";
import { api } from "@/lib/api";

export default function SkillsPage() {
  const [skills, setSkills] = useState<any[]>([]);
  const [pipelines, setPipelines] = useState<any[]>([]);
  const [activeItem, setActiveItem] = useState<any>(null);
  const [tab, setTab] = useState<"skills" | "pipelines">("skills");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      api.getSkills().then((d) => setSkills(d.skills || [])),
      api.getPipelines().then((d) => setPipelines(d.pipelines || [])),
    ]).finally(() => setLoading(false));
  }, []);

  const items = tab === "skills" ? skills : pipelines;

  return (
    <>
      <Header title="Skills & Pipelines" />

      <div className="tab-bar">
        <button className={`btn ${tab === "skills" ? "btn-primary" : ""}`} onClick={() => { setTab("skills"); setActiveItem(null); }}>
          Skills ({skills.length})
        </button>
        <button className={`btn ${tab === "pipelines" ? "btn-primary" : ""}`} onClick={() => { setTab("pipelines"); setActiveItem(null); }}>
          Pipelines ({pipelines.length})
        </button>
      </div>

      {loading ? (
        <div className="loading">Loading...</div>
      ) : items.length === 0 ? (
        <EmptyState title={`No ${tab}`} description={`No ${tab} found in projects/shared/${tab}/.`} />
      ) : (
        <div className="detail-grid" style={{ gridTemplateColumns: "240px 1fr" }}>
          {/* File list */}
          <div className="detail-sidebar">
            <div className="dash-panel">
              {items.map((item: any) => (
                <div
                  key={item.name}
                  className={`tree-item ${activeItem?.name === item.name ? "tree-item-active" : ""}`}
                  style={{ padding: "6px 12px", cursor: "pointer" }}
                  onClick={() => setActiveItem(item)}
                >
                  <span className="tree-icon">
                    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5">
                      <path d="M3 1h4l3 3v7a1 1 0 01-1 1H3a1 1 0 01-1-1V2a1 1 0 011-1z" />
                      <path d="M7 1v3h3" />
                    </svg>
                  </span>
                  <span className="tree-label">{item.name}{item.kind === "doc" ? ".md" : ".toml"}</span>
                  {item.source && item.source !== "shared" && (
                    <span className="text-hint">{item.source}</span>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Content viewer */}
          <div className="detail-main">
            {activeItem ? (
              <div className="dash-panel">
                <div className="dash-panel-header">
                  <span className="dash-panel-title">{activeItem.name}{activeItem.kind === "doc" ? ".md" : ".toml"}</span>
                  {activeItem.source && (
                    <span style={{ fontSize: "var(--font-size-xs)", color: "var(--text-muted)" }}>
                      {activeItem.source === "shared" ? "Shared skill" : `Project: ${activeItem.source}`}
                    </span>
                  )}
                </div>
                <pre style={{
                  padding: "var(--space-4)",
                  fontFamily: "var(--font-mono)",
                  fontSize: "var(--font-size-sm)",
                  color: "var(--text-secondary)",
                  lineHeight: "1.6",
                  whiteSpace: "pre-wrap",
                  wordWrap: "break-word",
                  margin: 0,
                  maxHeight: "600px",
                  overflowY: "auto",
                }}>
                  {activeItem.content}
                </pre>
              </div>
            ) : (
              <EmptyState title="Select a file" description={`Click a ${tab === "skills" ? "skill" : "pipeline"} to view its configuration.`} />
            )}
          </div>
        </div>
      )}
    </>
  );
}
