import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "@/lib/api";
import { useUIStore } from "@/store/ui";
import { useDaemonStore } from "@/store/daemon";
import BlockAvatar from "./BlockAvatar";

interface Workspace {
  name: string;
}

export default function WorkspaceSwitcher() {
  const activeCompany = useUIStore((s) => s.activeCompany);
  const setActiveCompany = useUIStore((s) => s.setActiveCompany);
  const agents = useDaemonStore((s) => s.agents);
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Derive companies from top-level agents (no parent) or companies API
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);

  useEffect(() => {
    // Try companies API first
    api
      .getCompanies()
      .then((data: any) => {
        const raw = data?.companies || data?.projects || data?.agent_spawns || [];
        const items = Array.isArray(raw) ? raw : [];
        if (items.length > 0) {
          setWorkspaces(
            items.map((c: any) => ({ name: c.name || c.company || "" })).filter((w: Workspace) => w.name),
          );
          return;
        }
        // Fallback: derive from top-level agents (projects are root-level in the tree)
        deriveFromAgents();
      })
      .catch(() => {
        deriveFromAgents();
      });
  }, [agents]);

  const deriveFromAgents = () => {
    // Root agents (no parent) that look like projects
    const roots = agents.filter(
      (a) => !a.parent_id && a.project,
    );
    const projectNames = [...new Set(roots.map((a) => a.project!).filter(Boolean))];
    if (projectNames.length > 0) {
      setWorkspaces(projectNames.map((name) => ({ name })));
    } else {
      // Last fallback: use root agent names as workspaces
      const rootNames = agents
        .filter((a) => !a.parent_id)
        .map((a) => a.name)
        .filter(Boolean);
      const unique = [...new Set(rootNames)];
      if (unique.length > 0) {
        setWorkspaces(unique.map((name) => ({ name })));
      }
    }
  };

  // Auto-select first company
  useEffect(() => {
    if (!activeCompany && workspaces.length > 0) {
      setActiveCompany(workspaces[0].name);
    }
  }, [workspaces, activeCompany, setActiveCompany]);

  // Click outside to close
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const displayName = activeCompany || "aeqi";

  return (
    <div className="ws-switcher" ref={ref}>
      <div className="ws-trigger">
        <span className="ws-brand" onClick={() => navigate("/")}>
          <BlockAvatar name={displayName} size={22} />
        </span>
        <div className="ws-trigger-text" onClick={() => navigate("/")}>
          <span className="ws-trigger-name">{displayName}</span>
          <span className="ws-trigger-plan">
            {localStorage.getItem("aeqi_company_tagline") || "The agent runtime."}
          </span>
        </div>
        <button
          className="ws-chevron-btn"
          onClick={() => setOpen(!open)}
          title="Switch company"
        >
          <svg
            width="12"
            height="12"
            viewBox="0 0 12 12"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
          >
            <path d="M4 3l2-1.5L8 3" />
            <path d="M4 9l2 1.5L8 9" />
          </svg>
        </button>
      </div>

      {open && (
        <div className="ws-dropdown">
          <div className="ws-dropdown-label">Companies</div>
          {workspaces.map((w) => (
            <button
              key={w.name}
              className={`ws-option ${w.name === activeCompany ? "active" : ""}`}
              onClick={() => {
                setActiveCompany(w.name);
                setOpen(false);
              }}
            >
              <BlockAvatar name={w.name} size={20} />
              <span className="ws-option-name">{w.name}</span>
              {w.name === activeCompany && (
                <svg
                  width="12"
                  height="12"
                  viewBox="0 0 12 12"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                >
                  <path d="M2.5 6l2.5 2.5 4.5-5" />
                </svg>
              )}
            </button>
          ))}

          <button
            className="ws-create-btn"
            onClick={() => {
              setOpen(false);
              navigate("/new");
            }}
          >
            + New company
          </button>
        </div>
      )}
    </div>
  );
}
