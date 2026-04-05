import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "@/lib/api";
import "@/styles/modals.css";

interface Props {
  open: boolean;
  onClose: () => void;
}

function slugify(input: string): string {
  return input
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function parseCsv(input: string): string[] {
  return input
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

export default function CreatePromptModal({ open, onClose }: Props) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [tagsInput, setTagsInput] = useState("");
  const [toolsInput, setToolsInput] = useState("");
  const [body, setBody] = useState("");

  const [showPreview, setShowPreview] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState(false);

  const surfaceRef = useRef<HTMLDivElement>(null);

  const slug = useMemo(() => slugify(name), [name]);
  const tags = useMemo(() => parseCsv(tagsInput), [tagsInput]);
  const tools = useMemo(() => parseCsv(toolsInput), [toolsInput]);

  const generatedContent = useMemo(() => {
    const lines: string[] = ["---"];
    lines.push(`name: ${slug || "untitled"}`);
    lines.push(`description: ${description || ""}`);
    if (tags.length > 0) {
      lines.push(`tags: [${tags.join(", ")}]`);
    }
    if (tools.length > 0) {
      lines.push(`tools: [${tools.join(", ")}]`);
    }
    lines.push("---");
    lines.push("");
    lines.push(body);
    return lines.join("\n");
  }, [slug, description, tags, tools, body]);

  // Reset form on open
  useEffect(() => {
    if (open) {
      setName("");
      setDescription("");
      setTagsInput("");
      setToolsInput("");
      setBody("");
      setError("");
      setSuccess(false);
      setShowPreview(true);
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
    if (!slug) {
      setError("Name is required.");
      return;
    }
    if (!description.trim()) {
      setError("Description is required.");
      return;
    }
    if (!body.trim()) {
      setError("Body is required.");
      return;
    }
    setSubmitting(true);
    setError("");
    try {
      await api.createPrompt({
        project: "shared",
        name: slug,
        content: generatedContent,
      });
      setSuccess(true);
      setTimeout(() => onClose(), 600);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create prompt.");
    } finally {
      setSubmitting(false);
    }
  };

  if (!open) return null;

  const canSubmit = slug && description.trim() && body.trim() && !submitting && !success;

  return (
    <div className="modal-backdrop" onClick={handleBackdropClick}>
      <div className="modal-surface" ref={surfaceRef}>
        <h2 className="modal-title">New Prompt</h2>

        {error && <div className="modal-error">{error}</div>}
        {success && <div className="modal-success">Prompt created successfully.</div>}

        {/* Name */}
        <div className="modal-field">
          <label className="modal-label">Name *</label>
          <input
            className="modal-input"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Prompt Name"
          />
          {name && slug !== name.trim().toLowerCase() && (
            <div className="modal-hint">Filename: {slug}.md</div>
          )}
        </div>

        {/* Description */}
        <div className="modal-field">
          <label className="modal-label">Description *</label>
          <input
            className="modal-input"
            type="text"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="One-line description of this prompt"
          />
        </div>

        {/* Tags */}
        <div className="modal-field">
          <label className="modal-label">Tags</label>
          <input
            className="modal-input"
            type="text"
            value={tagsInput}
            onChange={(e) => setTagsInput(e.target.value)}
            placeholder="workflow, rust, planning"
          />
          {tags.length > 0 && (
            <div className="cpm-tags-row">
              {tags.map((tag) => (
                <span key={tag} className="cpm-tag-chip">
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Tools */}
        <div className="modal-field">
          <label className="modal-label">Tools</label>
          <input
            className="modal-input"
            type="text"
            value={toolsInput}
            onChange={(e) => setToolsInput(e.target.value)}
            placeholder="shell, read_file, write_file"
          />
          {tools.length > 0 && (
            <div className="cpm-tags-row">
              {tools.map((tool) => (
                <span key={tool} className="cpm-tag-chip">
                  {tool}
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Body */}
        <div className="modal-field">
          <label className="modal-label">Body *</label>
          <textarea
            className="modal-textarea cpm-body-textarea"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Write the system prompt content here..."
            rows={8}
          />
        </div>

        {/* Preview */}
        <button
          className="cpm-preview-toggle"
          onClick={() => setShowPreview((v) => !v)}
          type="button"
        >
          {showPreview ? "\u25BC" : "\u25B6"} Preview
        </button>
        {showPreview && (
          <div className="cpm-preview">{generatedContent}</div>
        )}

        {/* Submit */}
        <div className="modal-actions">
          <button
            className="modal-btn-primary"
            onClick={handleSubmit}
            disabled={!canSubmit}
          >
            {submitting ? "Creating..." : "Create Prompt"}
          </button>
        </div>
      </div>
    </div>
  );
}
