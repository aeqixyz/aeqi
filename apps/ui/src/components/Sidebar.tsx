import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import type { PersistentAgent, Department } from "@/lib/types";

function Chevron({ expanded }: { expanded: boolean }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      style={{
        transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
        transition: "transform 0.15s ease",
        flexShrink: 0,
      }}
    >
      <path
        d="M5 3.5L8.5 7L5 10.5"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

interface DeptNode {
  dept: Department;
  agents: PersistentAgent[];
  children: DeptNode[];
}

function buildTree(
  departments: Department[],
  agents: PersistentAgent[],
  parentId: string | null
): DeptNode[] {
  return departments
    .filter((d) => (d.parent_id || null) === parentId)
    .map((dept) => ({
      dept,
      agents: agents.filter((a) => a.department_id === dept.id),
      children: buildTree(departments, agents, dept.id),
    }))
    .filter((n) => n.agents.length > 0 || n.children.length > 0);
}

function DeptGroupView({
  node,
  depth,
  selectedAgent,
  collapsed,
  onSelectAgent,
  onSelectDept,
  onToggle,
}: {
  node: DeptNode;
  depth: number;
  selectedAgent: string | null;
  collapsed: Record<string, boolean>;
  onSelectAgent: (name: string) => void;
  onSelectDept: (id: string) => void;
  onToggle: (id: string, e: React.MouseEvent) => void;
}) {
  const isCollapsed = collapsed[node.dept.id] ?? false;
  const isDeptActive = selectedAgent === `dept:${node.dept.id}`;

  // Each depth level gets slightly more bg opacity
  const bg = `rgba(255,255,255,${0.02 + depth * 0.01})`;

  return (
    <div className="dept-group" style={{ background: bg }}>
      <div
        className={`dept-name${isDeptActive ? " active" : ""}`}
        onClick={() => onSelectDept(node.dept.id)}
      >
        <span className="dept-name-label">{node.dept.name}</span>
        <span className="dept-chevron" onClick={(e) => onToggle(node.dept.id, e)}>
          <Chevron expanded={!isCollapsed} />
        </span>
      </div>
      {!isCollapsed && (
        <>
          {node.agents.map((agent) => (
            <div
              key={agent.id}
              className={`agent-row dept-agent${selectedAgent === agent.name ? " active" : ""}`}
              onClick={() => onSelectAgent(agent.name)}
            >
              {agent.display_name || agent.name}
            </div>
          ))}
          {node.children.map((child) => (
            <DeptGroupView
              key={child.dept.id}
              node={child}
              depth={depth + 1}
              selectedAgent={selectedAgent}
              collapsed={collapsed}
              onSelectAgent={onSelectAgent}
              onSelectDept={onSelectDept}
              onToggle={onToggle}
            />
          ))}
        </>
      )}
    </div>
  );
}

export default function AgentNav() {
  const navigate = useNavigate();
  const channel = useChatStore((s) => s.channel);
  const selectedAgent = useChatStore((s) => s.selectedAgent);
  const setSelectedAgent = useChatStore((s) => s.setSelectedAgent);
  const [agents, setAgents] = useState<PersistentAgent[]>([]);
  const [departments, setDepartments] = useState<Department[]>([]);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  useEffect(() => {
    const load = () => {
      api.getAgents().then((d: any) => {
        const list = d.agents || d.registry || [];
        setAgents(list.filter((a: PersistentAgent) => a.status === "Active" || a.status === "active"));
      }).catch(() => {});

      api.getDepartments?.().then((d: any) => {
        setDepartments(d.departments || []);
      }).catch(() => {});
    };
    load();
    const interval = setInterval(load, 20000);
    return () => clearInterval(interval);
  }, [channel]);

  const filtered = channel
    ? agents.filter((a) => a.project === channel || !a.project)
    : agents.filter((a) => !a.project);

  const filteredDepts = departments.filter((d) => !channel || d.project === channel);
  const tree = buildTree(filteredDepts, filtered, null);

  // Root agents: not in any department
  const allDeptAgentIds = new Set<string>();
  const collectIds = (nodes: DeptNode[]) => {
    for (const n of nodes) {
      n.agents.forEach((a) => allDeptAgentIds.add(a.id));
      collectIds(n.children);
    }
  };
  collectIds(tree);
  const rootAgents = filtered.filter((a) => !allDeptAgentIds.has(a.id));

  const scopeName = channel || "AEQI";

  const toggleDept = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setCollapsed((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const handleSelectAgent = (name: string) => {
    setSelectedAgent(name);
    const currentPath = window.location.pathname;
    const base = currentPath === "/login" ? "/" : currentPath;
    navigate(`${base}?agent=${encodeURIComponent(name)}`);
  };

  const handleSelectDept = (id: string) => {
    setSelectedAgent(`dept:${id}`);
    navigate(`/departments/${id}`);
  };

  return (
    <nav className="agent-nav">
      <div
        className={`agent-row scope-header${!selectedAgent ? " active" : ""}`}
        onClick={() => { setSelectedAgent(null); navigate(window.location.pathname); }}
      >
        {scopeName}
      </div>

      <div className="agent-nav-sep" />

      {rootAgents.map((agent) => (
        <div
          key={agent.id}
          className={`agent-row${selectedAgent === agent.name ? " active" : ""}`}
          onClick={() => handleSelectAgent(agent.name)}
        >
          {agent.display_name || agent.name}
        </div>
      ))}

      {tree.map((node) => (
        <DeptGroupView
          key={node.dept.id}
          node={node}
          depth={0}
          selectedAgent={selectedAgent}
          collapsed={collapsed}
          onSelectAgent={handleSelectAgent}
          onSelectDept={handleSelectDept}
          onToggle={toggleDept}
        />
      ))}

      {/* Bottom — pinned */}
      <div className="agent-nav-bottom">
        <div className="agent-nav-sep" />
        <div className="agent-nav-add" onClick={() => navigate("/agents")}>+</div>
      </div>
    </nav>
  );
}
