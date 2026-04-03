import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useChatStore } from "@/store/chat";
import { api } from "@/lib/api";
import CompanyPatternIcon from "./CompanyPatternIcon";

export default function CompanyRail() {
  const navigate = useNavigate();
  const channel = useChatStore((s) => s.channel);
  const setChannel = useChatStore((s) => s.setChannel);
  const [companies, setCompanies] = useState<any[]>([]);
  const [activeCounts, setActiveCounts] = useState<Record<string, number>>({});

  useEffect(() => {
    const load = () => {
      api.getCompanies().then((d) => setCompanies(d.companies || [])).catch(() => {});
      api.getTasks({ status: "in_progress" }).then((d) => {
        const counts: Record<string, number> = {};
        for (const t of d.tasks || []) {
          counts[t.company] = (counts[t.company] || 0) + 1;
        }
        setActiveCounts(counts);
      }).catch(() => {});
    };
    load();
    const interval = setInterval(load, 15000);
    return () => clearInterval(interval);
  }, []);

  const selectedCompany = channel ?? null;

  return (
    <div className="rail">
      <div className="rail-inner">
        <div
          className={`rail-icon rail-home${!channel ? " active" : ""}`}
          onClick={() => { setChannel(null); navigate("/"); }}
          title="AEQI"
        >
          Æ
        </div>

        <div className="rail-separator" />

        <div className="rail-add" title="New company" onClick={() => {}}>+</div>

        {companies.map((p) => {
          const isSelected = selectedCompany === p.name;
          const hasActive = (activeCounts[p.name] || 0) > 0;

          return (
            <div key={p.name} className="rail-project-wrapper">
              <button
                className="rail-project-btn"
                onClick={() => { setChannel(p.name); navigate("/"); }}
                title={p.name}
              >
                <CompanyPatternIcon name={p.name} selected={isSelected} />
                {hasActive && (
                  <span className="rail-live-dot">
                    <span className="rail-live-dot-pulse" />
                    <span className="rail-live-dot-core" />
                  </span>
                )}
              </button>
            </div>
          );
        })}
        <div className="rail-settings" title="Settings">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="8" cy="8" r="2" />
            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" />
          </svg>
        </div>
      </div>
    </div>
  );
}
