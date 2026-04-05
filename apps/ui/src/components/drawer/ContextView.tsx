import { useEffect, useState, useRef, useCallback, useMemo } from "react";
import { useDaemonStore } from "@/store/daemon";
import { api } from "@/lib/api";
import type { Agent } from "@/lib/types";
import BlockAvatar from "../BlockAvatar";

interface PromptEntry {
  content: string;
  position: "system" | "prepend" | "append";
  scope: "self" | "descendants";
  tools?: { allow?: string[]; deny?: string[] };
}

interface PromptChainNode {
  agent_name: string;
  agent_id: string;
  prompts: PromptEntry[];
}

function positionBadge(pos: string) {
  const colors: Record<string, string> = {
    system: "var(--text-primary)",
    prepend: "var(--info)",
    append: "var(--text-muted)",
  };
  return (
    <span
      className="ctx-prompt-badge"
      style={{ color: colors[pos] || "var(--text-muted)" }}
    >
      {pos}
    </span>
  );
}

function scopeBadge(scope: string) {
  return (
    <span className="ctx-prompt-badge ctx-prompt-scope">
      {scope === "descendants" ? "\u2193 inherited" : "\u25cf self"}
    </span>
  );
}

// --- Prompts Section ---

function PromptsSection({ agentName }: { agentName: string }) {
  const agents = useDaemonStore((s) => s.agents);
  const [expanded, setExpanded] = useState(false);

  const chain: PromptChainNode[] = useMemo(() => {
    const byName = new Map<string, Agent>();
    const byId = new Map<string, Agent>();
    for (const a of agents) {
      byName.set(a.name, a);
      byId.set(a.id, a);
    }

    const ancestors: Agent[] = [];
    let current = byName.get(agentName);
    while (current) {
      ancestors.unshift(current);
      if (current.parent_id) {
        current = byId.get(current.parent_id);
      } else {
        break;
      }
    }

    return ancestors.map((a) => ({
      agent_name: a.name,
      agent_id: a.id,
      prompts: (a as any).prompts || [],
    }));
  }, [agents, agentName]);

  const totalPrompts = chain.reduce(
    (sum, node) => sum + node.prompts.length,
    0,
  );

  return (
    <div className="ctx-section">
      <button
        className="ctx-prompts-summary"
        onClick={() => setExpanded((p) => !p)}
      >
        <span>
          {chain.length} agent{chain.length !== 1 ? "s" : ""}, {totalPrompts}{" "}
          prompt{totalPrompts !== 1 ? "s" : ""}
        </span>
        <svg
          width="12"
          height="12"
          viewBox="0 0 12 12"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          style={{
            transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
            transition: "transform 150ms ease",
          }}
        >
          <path d="M4 2l4 4-4 4" />
        </svg>
      </button>

      {expanded && (
        <div className="ctx-prompt-chain">
          {chain.length === 0 ? (
            <div className="ctx-empty-state">No prompts configured</div>
          ) : (
            chain.map((node) => (
              <div key={node.agent_id} className="ctx-prompt-node">
                <div className="ctx-prompt-agent">
                  <BlockAvatar name={node.agent_name} size={18} />
                  <span className="ctx-prompt-agent-name">
                    {node.agent_name}
                  </span>
                </div>
                {node.prompts.length === 0 ? (
                  <div className="ctx-prompt-empty">no prompts</div>
                ) : (
                  node.prompts.map((entry, i) => (
                    <div key={i} className="ctx-prompt-entry">
                      <div className="ctx-prompt-meta">
                        {positionBadge(entry.position)}
                        {scopeBadge(entry.scope)}
                      </div>
                      <pre
                        className="ctx-pre"
                        style={{ maxHeight: 150, overflow: "auto" }}
                      >
                        {entry.content}
                      </pre>
                    </div>
                  ))
                )}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}

// --- Insights Section ---

function InsightsSection({ agentName }: { agentName: string }) {
  const [insights, setInsights] = useState<any[]>([]);
  const [search, setSearch] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const fetchInsights = useCallback(
    (query: string) => {
      api
        .getMemories({ query: query || agentName, limit: 20 })
        .then((d: any) => setInsights(d.memories || d.items || []))
        .catch(() => setInsights([]));
    },
    [agentName],
  );

  // Fetch on mount and when agentName changes
  useEffect(() => {
    fetchInsights(search);
  }, [agentName]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSearchChange = (value: string) => {
    setSearch(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => fetchInsights(value), 300);
  };

  // Cleanup debounce on unmount
  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  return (
    <div className="ctx-section">
      <input
        className="ctx-insights-search"
        placeholder="Search insights..."
        value={search}
        onChange={(e) => handleSearchChange(e.target.value)}
      />
      {insights.length > 0 ? (
        <div className="ctx-list">
          {insights.map((item: any) => {
            const id = item.id || item.key;
            const isExpanded = expandedId === id;
            const content = item.content || "";
            const preview =
              content.length > 120 ? content.slice(0, 120) + "\u2026" : content;

            return (
              <div
                key={id}
                className="ctx-insight-row"
                onClick={() => setExpandedId(isExpanded ? null : id)}
                style={{ cursor: content.length > 120 ? "pointer" : "default" }}
              >
                <span className="ctx-insight-key">
                  {item.key || item.title || "insight"}
                </span>
                <span className="ctx-insight-content">
                  {isExpanded ? content : preview}
                </span>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="ctx-empty-state">No insights found</div>
      )}
    </div>
  );
}

// --- Main Component ---

export default function ContextView({ agentName }: { agentName: string }) {
  return (
    <div className="context-view">
      <PromptsSection agentName={agentName} />
      <InsightsSection agentName={agentName} />
    </div>
  );
}
