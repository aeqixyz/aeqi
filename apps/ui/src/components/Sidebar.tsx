import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import type { PersistentAgent } from "@/lib/types";

export default function AgentNav() {
  const navigate = useNavigate();
  const channel = useChatStore((s) => s.channel);
  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const setSelectedAgent = useChatStore((s) => s.setSelectedAgent);
  const [agents, setAgents] = useState<PersistentAgent[]>([]);

  useEffect(() => {
    const load = () => {
      api
        .getAgents()
        .then((d: any) => {
          const list = d.agents || d.registry || [];
          setAgents(list.filter((a: PersistentAgent) => a.status === "Active" || a.status === "active"));
        })
        .catch(() => {});
    };
    load();
    const interval = setInterval(load, 20000);
    return () => clearInterval(interval);
  }, [channel]);

  // Filter agents by selected project
  const filtered = channel
    ? agents.filter((a) => a.project === channel || !a.project)
    : agents.filter((a) => !a.project);

  const handleSelect = (agent: PersistentAgent) => {
    setSelectedAgent(agent.name);
    navigate("/");
  };

  return (
    <nav className="nav">
      {/* Executive Assistant — always first */}
      <div className="nav-section">
        <button
          className={`nav-item${!selectedAgent ? " active" : ""}`}
          onClick={() => { setSelectedAgent(null); navigate("/"); }}
        >
          <span className="nav-agent-dot" />
          Executive Assistant
        </button>
      </div>

      {/* Other agents */}
      {filtered.length > 0 && (
        <div className="nav-section">
          <div className="nav-section-header">Agents</div>
          {filtered.map((agent) => (
            <button
              key={agent.id}
              className={`nav-item${selectedAgent === agent.name ? " active" : ""}`}
              onClick={() => handleSelect(agent)}
            >
              <span className="nav-agent-dot" />
              {agent.display_name || agent.name}
            </button>
          ))}
        </div>
      )}

      {/* Add agent */}
      <button
        className="nav-add"
        onClick={() => navigate("/agents")}
      >
        + New Agent
      </button>
    </nav>
  );
}
