import { useEffect, useState } from "react";
import Header from "@/components/Header";
import EmptyState from "@/components/EmptyState";
import { api } from "@/lib/api";

const CATEGORY_COLORS: Record<string, string> = {
  fact: "#3b82f6",
  procedure: "#8b5cf6",
  preference: "#f59e0b",
  context: "#06b6d4",
  evergreen: "#22c55e",
  insight: "#22c55e",
  decision: "#b08d57",
};

export default function KnowledgePage() {
  const [companyList, setCompanyList] = useState<any[]>([]);
  const [allCompanies, setAllCompanies] = useState<any[]>([]);
  const [selectedCompany, setSelectedCompany] = useState<string>("");
  const [items, setItems] = useState<any[]>([]);
  const [search, setSearch] = useState("");
  const [sourceFilter, setSourceFilter] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [newEntry, setNewEntry] = useState({ key: "", content: "", category: "fact", scope: "domain" });
  const [creating, setCreating] = useState(false);

  // Load company list with memory counts.
  useEffect(() => {
    api.getMemories().then((d) => {
      setCompanyList(d.companies || []);
      const first = (d.companies || []).find((p: any) => p.count > 0);
      if (first) setSelectedCompany(first.company);
    }).catch(() => {});
    api.getCompanies().then((d) => setAllCompanies(d.companies || [])).catch(() => {});
    setLoading(false);
  }, []);

  // Load knowledge for selected company.
  useEffect(() => {
    if (!selectedCompany) return;
    setLoading(true);
    // Fetch both memories and notes.
    api.getChannelKnowledge({
      company: selectedCompany,
      query: search || undefined,
      limit: 100,
    }).then((d) => {
      let results = d.items || [];
      if (sourceFilter) {
        results = results.filter((item: any) => item.source === sourceFilter);
      }
      setItems(results);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, [selectedCompany, search, sourceFilter]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedCompany || !newEntry.key || !newEntry.content) return;
    setCreating(true);
    try {
      await api.storeKnowledge({
        company: selectedCompany,
        ...newEntry,
      });
      setNewEntry({ key: "", content: "", category: "fact", scope: "domain" });
      setShowCreate(false);
      // Refresh.
      const d = await api.getChannelKnowledge({ company: selectedCompany, limit: 100 });
      setItems(d.items || []);
    } catch { /* ignore */ }
    setCreating(false);
  };

  const handleDelete = async (id: string) => {
    if (!selectedCompany) return;
    await api.deleteKnowledge({ company: selectedCompany, id });
    setItems(items.filter((item) => item.id !== id));
    setExpanded(null);
  };

  const memoryCount = items.filter((i) => i.source === "memory").length;
  const notesCount = items.filter((i) => i.source === "notes" || i.source === "notes").length;

  return (
    <>
      <Header
        title="Knowledge"
        actions={
          <button className="btn btn-primary" onClick={() => setShowCreate(!showCreate)}>
            {showCreate ? "Cancel" : "+ New Entry"}
          </button>
        }
      />

      {/* Create form */}
      {showCreate && selectedCompany && (
        <form className="dash-panel form-panel" onSubmit={handleCreate}>
          <div className="form-row">
            <input
              className="filter-input flex-1"
              placeholder="Key (e.g., signal-latency-target)"
              value={newEntry.key}
              onChange={(e) => setNewEntry({ ...newEntry, key: e.target.value })}
              required
            />
            <select className="filter-select" value={newEntry.category} onChange={(e) => setNewEntry({ ...newEntry, category: e.target.value })}>
              <option value="fact">Fact</option>
              <option value="procedure">Procedure</option>
              <option value="preference">Preference</option>
              <option value="context">Context</option>
              <option value="evergreen">Evergreen</option>
            </select>
            <select className="filter-select" value={newEntry.scope} onChange={(e) => setNewEntry({ ...newEntry, scope: e.target.value })}>
              <option value="domain">Domain</option>
              <option value="system">System</option>
            </select>
          </div>
          <textarea
            className="filter-input form-textarea"
            placeholder="Content..."
            value={newEntry.content}
            onChange={(e) => setNewEntry({ ...newEntry, content: e.target.value })}
            required
          />
          <button className="btn btn-primary" type="submit" disabled={creating}>
            {creating ? "Storing..." : "Store Knowledge"}
          </button>
        </form>
      )}

      {/* Filters */}
      <div className="filters">
        <select
          className="filter-select"
          value={selectedCompany}
          onChange={(e) => setSelectedCompany(e.target.value)}
        >
          <option value="">Select company...</option>
          {(companyList.length > 0 ? companyList : allCompanies).map((p: any) => (
            <option key={p.company || p.name} value={p.company || p.name}>
              {p.company || p.name} {p.count ? `(${p.count})` : ""}
            </option>
          ))}
        </select>
        <select
          className="filter-select"
          value={sourceFilter}
          onChange={(e) => setSourceFilter(e.target.value)}
        >
          <option value="">All sources</option>
          <option value="memory">Memory ({memoryCount})</option>
          <option value="notes">Notes ({notesCount})</option>
        </select>
        <input
          className="filter-input flex-1"
          placeholder="Search knowledge..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <span className="filter-count">
          {items.length} entries
        </span>
      </div>

      {!selectedCompany ? (
        <EmptyState title="Select a company" description="Choose a company to browse its knowledge base." />
      ) : loading ? (
        <div className="loading">Loading knowledge...</div>
      ) : items.length === 0 ? (
        <EmptyState title="No knowledge" description={search ? "No entries match your search." : "No knowledge stored yet."} />
      ) : (
        <div>
          {items.map((item: any) => (
            <div
              key={item.id}
              className={`memory-entry ${expanded === item.id ? "memory-entry-expanded" : ""}`}
              onClick={() => setExpanded(expanded === item.id ? null : item.id)}
              style={{ cursor: "pointer" }}
            >
              <div className="memory-header">
                <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", flex: 1, minWidth: 0 }}>
                  <span className={`knowledge-source-icon ${item.source === "memory" ? "memory" : "notes"}`}>
                    {item.source === "memory" ? "M" : "B"}
                  </span>
                  <code className="memory-key">{item.key}</code>
                </div>
                <div className="memory-tags">
                  {item.category && (
                    <span className="memory-category" style={{ color: CATEGORY_COLORS[item.category] || "var(--text-muted)" }}>
                      {item.category}
                    </span>
                  )}
                  {item.scope && (
                    <span className="memory-scope">{item.scope}</span>
                  )}
                </div>
              </div>
              {expanded === item.id ? (
                <>
                  <div className="memory-content" style={{ whiteSpace: "pre-wrap" }}>{item.content}</div>
                  <div className="memory-meta">
                    <span>{new Date(item.created_at).toLocaleString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</span>
                    {item.source === "memory" && (
                      <button
                        className="btn btn-2xs"
                        style={{ color: "var(--error)" }}
                        onClick={(e) => { e.stopPropagation(); handleDelete(item.id); }}
                      >
                        Delete
                      </button>
                    )}
                  </div>
                </>
              ) : (
                <div className="knowledge-preview">
                  {item.content.slice(0, 120)}{item.content.length > 120 ? "..." : ""}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  );
}
