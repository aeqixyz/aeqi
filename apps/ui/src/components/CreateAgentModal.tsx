import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { useDaemonStore } from "@/store/daemon";
import "@/styles/modals.css";

interface Skill {
  name: string;
  tags?: string[];
}

interface Props {
  open: boolean;
  onClose: () => void;
}

export default function CreateAgentModal({ open, onClose }: Props) {
  const agents = useDaemonStore((s) => s.agents);
  const fetchAgents = useDaemonStore((s) => s.fetchAgents);

  const [templates, setTemplates] = useState<string[]>([]);
  const [loadingTemplates, setLoadingTemplates] = useState(false);
  const [useFallback, setUseFallback] = useState(false);

  const [template, setTemplate] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [parentId, setParentId] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState(false);

  const surfaceRef = useRef<HTMLDivElement>(null);

  // Fetch identity templates from skills
  useEffect(() => {
    if (!open) return;
    setLoadingTemplates(true);
    api
      .getSkills()
      .then((data) => {
        const skills: Skill[] = data?.skills || data || [];
        const identity = skills.filter(
          (s) => Array.isArray(s.tags) && s.tags.includes("identity"),
        );
        if (identity.length > 0) {
          setTemplates(identity.map((s) => s.name));
          setUseFallback(false);
        } else {
          setTemplates([]);
          setUseFallback(true);
        }
      })
      .catch(() => {
        setTemplates([]);
        setUseFallback(true);
      })
      .finally(() => setLoadingTemplates(false));
  }, [open]);

  // Reset form state when opening
  useEffect(() => {
    if (open) {
      setTemplate("");
      setDisplayName("");
      setParentId("");
      setSystemPrompt("");
      setError("");
      setSuccess(false);
    }
  }, [open]);

  // Escape key
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (surfaceRef.current && !surfaceRef.current.contains(e.target as Node)) {
        onClose();
      }
    },
    [onClose],
  );

  const handleSubmit = async () => {
    if (!template.trim()) {
      setError("Template is required.");
      return;
    }
    setSubmitting(true);
    setError("");
    try {
      await api.spawnAgent({
        template: template.trim(),
        ...(parentId ? { parent_id: parentId } : {}),
      });
      setSuccess(true);
      await fetchAgents();
      setTimeout(() => onClose(), 600);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to spawn agent.");
    } finally {
      setSubmitting(false);
    }
  };

  if (!open) return null;

  return (
    <div className="modal-backdrop" onClick={handleBackdropClick}>
      <div className="modal-surface" ref={surfaceRef}>
        <h2 className="modal-title">New Agent</h2>

        {error && <div className="modal-error">{error}</div>}
        {success && <div className="modal-success">Agent spawned successfully.</div>}

        {/* Template */}
        <div className="modal-field">
          <label className="modal-label">Template *</label>
          {loadingTemplates ? (
            <div className="modal-hint">Loading templates...</div>
          ) : useFallback ? (
            <>
              <input
                className="modal-input"
                type="text"
                value={template}
                onChange={(e) => setTemplate(e.target.value)}
                placeholder="e.g. researcher"
              />
              <div className="cam-template-fallback-hint">
                No identity templates found. Enter a template name manually.
              </div>
            </>
          ) : (
            <select
              className="modal-select"
              value={template}
              onChange={(e) => setTemplate(e.target.value)}
            >
              <option value="">Select template...</option>
              {templates.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          )}
        </div>

        {/* Display name */}
        <div className="modal-field">
          <label className="modal-label">Display Name</label>
          <input
            className="modal-input"
            type="text"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="Optional custom name"
          />
        </div>

        {/* Parent agent */}
        <div className="modal-field">
          <label className="modal-label">Parent Agent</label>
          <select
            className="modal-select"
            value={parentId}
            onChange={(e) => setParentId(e.target.value)}
          >
            <option value="">None</option>
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.display_name || a.name}
              </option>
            ))}
          </select>
        </div>

        {/* System prompt */}
        <div className="modal-field">
          <label className="modal-label">System Prompt</label>
          <textarea
            className="modal-textarea"
            value={systemPrompt}
            onChange={(e) => setSystemPrompt(e.target.value)}
            placeholder="Override or customize the template's system prompt..."
            rows={4}
          />
        </div>

        {/* Submit */}
        <div className="modal-actions">
          <button
            className="modal-btn-primary"
            onClick={handleSubmit}
            disabled={submitting || success || !template.trim()}
          >
            {submitting ? "Spawning..." : "Create Agent"}
          </button>
        </div>
      </div>
    </div>
  );
}
