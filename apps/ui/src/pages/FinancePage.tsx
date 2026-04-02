import { useEffect, useState } from "react";
import { api } from "@/lib/api";

export default function FinancePage() {
  const [cost, setCost] = useState<any>(null);

  useEffect(() => {
    api.getCost().then(setCost).catch(() => {});
  }, []);

  const spent = cost?.spent_today_usd ?? 0;
  const budget = cost?.daily_budget_usd ?? 0;
  const remaining = cost?.remaining_usd ?? 0;
  const pct = budget > 0 ? Math.min(100, (spent / budget) * 100) : 0;
  const projects = cost?.per_project || [];

  return (
    <div className="finance-page">
      <div className="finance-header">
        <h2 className="finance-title">Finance</h2>
        <p className="finance-meta">Cost tracking and budget utilization</p>
      </div>

      <div className="finance-stats">
        <div className="finance-stat">
          <span className="finance-stat-value">${spent.toFixed(2)}</span>
          <span className="finance-stat-label">Spent today</span>
        </div>
        <div className="finance-stat">
          <span className="finance-stat-value">${budget.toFixed(2)}</span>
          <span className="finance-stat-label">Daily budget</span>
        </div>
        <div className="finance-stat">
          <span className="finance-stat-value">${remaining.toFixed(2)}</span>
          <span className="finance-stat-label">Remaining</span>
        </div>
      </div>

      <div className="finance-bar-wrap">
        <div className="finance-bar">
          <div className="finance-bar-fill" style={{ width: `${pct}%` }} />
        </div>
        <span className="finance-bar-pct">{pct.toFixed(0)}%</span>
      </div>

      {projects.length > 0 && (
        <div className="finance-projects">
          <h3 className="finance-section-title">Per Project</h3>
          {projects.map((p: any) => (
            <div key={p.project} className="finance-project-row">
              <span className="finance-project-name">{p.project}</span>
              <span className="finance-project-spent">${(p.spent_usd ?? 0).toFixed(2)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
