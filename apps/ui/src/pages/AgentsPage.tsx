import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import Header from "@/components/Header";
import CreateAgentModal from "@/components/CreateAgentModal";
import { DataState } from "@/components/ui";
import { api } from "@/lib/api";

export default function AgentsPage() {
  const [agents, setAgents] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [modalOpen, setModalOpen] = useState(false);

  const loadAgents = () => {
    setLoading(true);
    api.getAgents().then((data) => {
      setAgents(data.agents || []);
      setLoading(false);
    }).catch(() => setLoading(false));
  };

  useEffect(() => {
    loadAgents();
  }, []);

  const handleModalClose = () => {
    setModalOpen(false);
    loadAgents();
  };

  return (
    <>
      <Header
        title="Agents"
        actions={
          <button className="btn btn-primary" onClick={() => setModalOpen(true)}>
            + New Agent
          </button>
        }
      />
      <DataState loading={loading} empty={agents.length === 0} emptyTitle="No agents" emptyDescription="No active agents found." loadingText="Loading agents...">
        <div className="cards-grid">
          {agents.map((a: any) => (
            <Link key={a.name} to={`/agents/${a.name}`} className="agent-card">
              <div className="agent-header">
                <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                  <div className="dash-agent-avatar">{a.name[0].toUpperCase()}</div>
                  <div>
                    <span className="agent-name">{a.name}</span>
                    <span className="agent-prefix">{a.prefix}</span>
                  </div>
                </div>
                <span className="agent-role">{a.role}</span>
              </div>
              {a.model && <div className="agent-model">{a.model}</div>}
              <div className="agent-expertise">
                {(a.expertise || []).map((e: string) => (
                  <span key={e} className="expertise-tag">{e}</span>
                ))}
              </div>
              {a.expertise_scores && a.expertise_scores.length > 0 && (
                <div className="agent-stats">
                  {a.expertise_scores.slice(0, 3).map((s: any, i: number) => (
                    <span key={i} style={{ fontSize: "var(--font-size-xs)", color: "var(--text-muted)" }}>
                      {(s.success_rate * 100).toFixed(0)}% ({s.total_tasks} tasks)
                    </span>
                  ))}
                </div>
              )}
            </Link>
          ))}
        </div>
      </DataState>
      <CreateAgentModal open={modalOpen} onClose={handleModalClose} />
    </>
  );
}
