import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import type { PersistentAgent } from "@/lib/types";

function AgentRow({
  name,
  isActive,
  onClick,
}: {
  name: string;
  isActive: boolean;
  onClick: () => void;
}) {
  const initial = name.charAt(0).toUpperCase();

  return (
    <div
      className={`agent-row${isActive ? " active" : ""}`}
      onClick={onClick}
    >
      <div className="agent-row-avatar">{initial}</div>
      <span className="agent-row-name">{name}</span>
    </div>
  );
}

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

  const filtered = channel
    ? agents.filter((a) => a.project === channel || !a.project)
    : agents.filter((a) => !a.project);

  return (
    <nav className="agent-nav">
      <AgentRow
        name="Executive Assistant"
        isActive={!selectedAgent}
        onClick={() => { setSelectedAgent(null); navigate("/"); }}
      />

      {filtered.map((agent) => (
        <AgentRow
          key={agent.id}
          name={agent.display_name || agent.name}
          isActive={selectedAgent === agent.name}
          onClick={() => { setSelectedAgent(agent.name); navigate("/"); }}
        />
      ))}

      <div
        className="agent-nav-add"
        onClick={() => navigate("/agents")}
      >
        +
      </div>
    </nav>
  );
}
