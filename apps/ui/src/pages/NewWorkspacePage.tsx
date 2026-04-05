import { useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "@/lib/api";
import { useUIStore } from "@/store/ui";
import BlockAvatar from "@/components/BlockAvatar";
import "@/styles/welcome.css";

export default function NewWorkspacePage() {
  const navigate = useNavigate();
  const setActiveCompany = useUIStore((s) => s.setActiveCompany);

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
      setActiveCompany(name.trim());
      if (tagline.trim()) {
        localStorage.setItem("aeqi_company_tagline", tagline.trim());
      }
      navigate("/agents");
    } catch (e: any) {
      setError(e?.message || "Failed to create company");
      setCreating(false);
    }
  };

  return (
    <div className="new-ws-page">
      <div className="new-ws-container new-ws-animate">
        <a className="new-ws-back" href="/" onClick={(e) => { e.preventDefault(); navigate("/"); }}>
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M8.5 3L4.5 7l4 4" /></svg>
          Back
        </a>

        <div className="new-ws-hero">
          {/* Avatar + Name inline */}
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
          <div className="new-ws-identity">
            <div className="new-ws-avatar" onClick={() => fileRef.current?.click()} title="Upload logo">
              {imageUrl ? (
                <img src={imageUrl} alt="" className="new-ws-avatar-img" />
              ) : (
                <BlockAvatar name={name || "W"} size={56} />
              )}
              <span className="new-ws-avatar-overlay">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M2 11l3.5-3.5L8 10l3-4 3 3M2 14h12" /></svg>
              </span>
            </div>
            <div className="new-ws-identity-fields">
              <input
                className="new-ws-name-input"
                placeholder="Company name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && name.trim()) handleCreate(); }}
                autoFocus
              />
              <input
                className="new-ws-tagline-input"
                placeholder="Add a tagline..."
                value={tagline}
                onChange={(e) => setTagline(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && name.trim()) handleCreate(); }}
              />
            </div>
          </div>
        </div>

        {error && <div className="new-ws-error">{error}</div>}

        <button
          className="new-ws-submit"
          onClick={handleCreate}
          disabled={!name.trim() || creating}
        >
          {creating ? "Creating..." : (
            <>Create company <kbd className="new-ws-kbd">↵</kbd></>
          )}
        </button>

        <p className="new-ws-hint">
          You can change the name, logo, and tagline anytime from your company settings.
        </p>
      </div>
    </div>
  );
}
