import { useState, useEffect, useRef } from "react";
import { api } from "@/lib/api";
import { useUIStore } from "@/store/ui";

interface Workspace {
  name: string;
  prefix?: string;
}

export default function WorkspaceSwitcher() {
  const activeWorkspace = useUIStore((s) => s.activeWorkspace);
  const setActiveWorkspace = useUIStore((s) => s.setActiveWorkspace);
  const [open, setOpen] = useState(false);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Fetch workspaces
  useEffect(() => {
    api
      .getCompanies()
      .then((data: any) => {
        const items = data?.companies || data?.projects || [];
        setWorkspaces(items.map((c: any) => ({ name: c.name, prefix: c.prefix })));
        // Auto-select first if none active
        if (!activeWorkspace && items.length > 0) {
          setActiveWorkspace(items[0].name);
        }
      })
      .catch(() => {});
  }, []);

  // Click outside to close
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
        setCreating(false);
        setNewName("");
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleCreate = async () => {
    if (!newName.trim()) return;
    try {
      await api.createCompany({ name: newName.trim() });
      setActiveWorkspace(newName.trim());
      setWorkspaces((prev) => [...prev, { name: newName.trim() }]);
      setCreating(false);
      setNewName("");
      setOpen(false);
    } catch {}
  };

  const display = activeWorkspace || "Select workspace";

  return (
    <div className="ws-switcher" ref={ref}>
      <button className="ws-trigger" onClick={() => setOpen(!open)}>
        <span className="ws-brand">æ</span>
        <div className="ws-trigger-text">
          <span className="ws-trigger-name">{display}</span>
          <span className="ws-trigger-plan">aeqi.ai</span>
        </div>
        <svg
          className={`ws-chevron ${open ? "open" : ""}`}
          width="12"
          height="12"
          viewBox="0 0 12 12"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        >
          <path d="M3 5l3 3 3-3" />
        </svg>
      </button>

      {open && (
        <div className="ws-dropdown">
          <div className="ws-dropdown-label">Workspaces</div>
          {workspaces.map((w) => (
            <button
              key={w.name}
              className={`ws-option ${w.name === activeWorkspace ? "active" : ""}`}
              onClick={() => {
                setActiveWorkspace(w.name);
                setOpen(false);
              }}
            >
              <span className="ws-option-initial">
                {w.name.charAt(0).toUpperCase()}
              </span>
              <span className="ws-option-name">{w.name}</span>
              {w.name === activeWorkspace && (
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                  <path d="M2.5 6l2.5 2.5 4.5-5" />
                </svg>
              )}
            </button>
          ))}

          {creating ? (
            <div className="ws-create-form">
              <input
                ref={inputRef}
                className="ws-create-input"
                placeholder="Workspace name..."
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreate();
                  if (e.key === "Escape") {
                    setCreating(false);
                    setNewName("");
                  }
                }}
                autoFocus
              />
            </div>
          ) : (
            <button
              className="ws-create-btn"
              onClick={() => {
                setCreating(true);
                setTimeout(() => inputRef.current?.focus(), 50);
              }}
            >
              + New workspace
            </button>
          )}
        </div>
      )}
    </div>
  );
}
