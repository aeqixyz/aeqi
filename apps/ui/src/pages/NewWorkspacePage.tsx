import { useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "@/lib/api";
import { useUIStore } from "@/store/ui";
import BlockAvatar from "@/components/BlockAvatar";
import "@/styles/welcome.css";

export default function NewWorkspacePage() {
  const navigate = useNavigate();
  const setActiveWorkspace = useUIStore((s) => s.setActiveWorkspace);

  const [name, setName] = useState("");
  const [tagline, setTagline] = useState("");
  const [imageUrl, setImageUrl] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  const handleCreate = async () => {
    if (!name.trim() || creating) return;
    setCreating(true);
    setError("");
    try {
      await api.createCompany({ name: name.trim() });
      setActiveWorkspace(name.trim());
      if (tagline.trim()) {
        localStorage.setItem("aeqi_workspace_tagline", tagline.trim());
      }
      navigate("/agents");
    } catch (e: any) {
      setError(e?.message || "Failed to create workspace");
      setCreating(false);
    }
  };

  return (
    <div className="new-ws-page">
      <div className="new-ws-container">
        <a className="new-ws-back" href="/" onClick={(e) => { e.preventDefault(); navigate("/"); }}>
          &larr; Back
        </a>

        <h1 className="new-ws-title">Create a workspace</h1>
        <p className="new-ws-desc">
          A workspace is your company, project, or team — a self-contained
          environment with its own agents, quests, and knowledge.
        </p>

        <div className="new-ws-avatar-field">
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            style={{ display: "none" }}
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              const reader = new FileReader();
              reader.onload = () => setImageUrl(reader.result as string);
              reader.readAsDataURL(file);
              e.target.value = "";
            }}
          />
          <div className="new-ws-avatar" onClick={() => fileRef.current?.click()} title="Upload image">
            {imageUrl ? (
              <img src={imageUrl} alt="" className="new-ws-avatar-img" />
            ) : (
              <BlockAvatar name={name || "W"} size={64} />
            )}
            <span className="new-ws-avatar-overlay">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M2 11l3.5-3.5L8 10l3-4 3 3M2 14h12M13 2l1 1" /></svg>
            </span>
          </div>
        </div>

        <div className="new-ws-field">
          <label className="new-ws-label">Name</label>
          <input
            className="new-ws-input"
            placeholder="e.g. Acme Corp, my-project, research-lab"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
            autoFocus
          />
        </div>

        <div className="new-ws-field">
          <label className="new-ws-label">Tagline <span className="new-ws-optional">optional</span></label>
          <input
            className="new-ws-input"
            placeholder="A short description for your workspace"
            value={tagline}
            onChange={(e) => setTagline(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
          />
        </div>

        {error && <div className="new-ws-error">{error}</div>}

        <button
          className="new-ws-submit"
          onClick={handleCreate}
          disabled={!name.trim() || creating}
        >
          {creating ? "Creating..." : "Create workspace"}
        </button>
      </div>
    </div>
  );
}
