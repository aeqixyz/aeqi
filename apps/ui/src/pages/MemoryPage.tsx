import { useEffect, useState } from "react";
import Header from "@/components/Header";
import EmptyState from "@/components/EmptyState";
import { DataState } from "@/components/ui";
import { api } from "@/lib/api";

const CATEGORY_COLORS: Record<string, string> = {
  fact: "var(--info)",
  decision: "var(--accent)",
  preference: "var(--warning)",
  insight: "var(--success)",
};

const SCOPE_LABELS: Record<string, string> = {
  domain: "Domain",
  system: "System",
  self: "Self",
  personal: "Personal",
  session: "Session",
};

export default function MemoryPage() {
  const [companyList, setCompanyList] = useState<any[]>([]);
  const [selectedCompany, setSelectedCompany] = useState<string>("");
  const [memories, setMemories] = useState<any[]>([]);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);

  // Load companies with memory counts.
  useEffect(() => {
    api.getMemories().then((d) => {
      setCompanyList(d.companies || []);
      // Auto-select first company with memories.
      const first = (d.companies || []).find((p: any) => p.count > 0);
      if (first) setSelectedCompany(first.company);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  // Load memories for selected company.
  useEffect(() => {
    if (!selectedCompany) return;
    setLoading(true);
    api.getMemories({
      company: selectedCompany,
      query: search || undefined,
      limit: 100,
    }).then((d) => {
      setMemories(d.memories || []);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, [selectedCompany, search]);

  return (
    <>
      <Header title="Memory" />

      {/* Company selector + search */}
      <div className="filters">
        <select
          className="filter-select"
          value={selectedCompany}
          onChange={(e) => setSelectedCompany(e.target.value)}
        >
          <option value="">Select company...</option>
          {companyList.map((p: any) => (
            <option key={p.company} value={p.company}>
              {p.company} ({p.count} memories)
            </option>
          ))}
        </select>
        <input
          className="filter-input"
          style={{ flex: 1 }}
          placeholder="Search memories..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <span style={{ fontSize: "var(--font-size-xs)", color: "var(--text-muted)", alignSelf: "center" }}>
          {memories.length} results
        </span>
      </div>

      {!selectedCompany ? (
        <EmptyState title="Select a company" description="Choose a company to view its agent memories." />
      ) : (
        <DataState loading={loading} empty={memories.length === 0} emptyTitle="No memories" emptyDescription="No memories found." loadingText="Loading memories...">
          <div>
            {memories.map((m: any) => (
              <div key={m.id} className="memory-entry">
                <div className="memory-header">
                  <code className="memory-key">{m.key}</code>
                  <div className="memory-tags">
                    <span className="memory-category" style={{ color: CATEGORY_COLORS[m.category] || "var(--text-muted)" }}>
                      {m.category}
                    </span>
                    <span className="memory-scope">
                      {SCOPE_LABELS[m.scope] || m.scope}
                    </span>
                  </div>
                </div>
                <div className="memory-content">{m.content}</div>
                <div className="memory-meta">
                  {m.agent_id && <span>Agent: {m.agent_id}</span>}
                  <span>{new Date(m.created_at).toLocaleString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</span>
                </div>
              </div>
            ))}
          </div>
        </DataState>
      )}
    </>
  );
}
