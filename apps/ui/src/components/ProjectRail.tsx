import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useChatStore } from "@/store/chat";
import { useDaemonStore } from "@/store/daemon";
import { api } from "@/lib/api";
import ProjectPatternIcon from "./ProjectPatternIcon";

export default function ProjectRail() {
  const navigate = useNavigate();
  const channel = useChatStore((s) => s.channel);
  const setChannel = useChatStore((s) => s.setChannel);
  const status = useDaemonStore((s) => s.status);
  const daemonOk = status?.ok === true;
  const [projects, setProjects] = useState<any[]>([]);
  const [activeCounts, setActiveCounts] = useState<Record<string, number>>({});

  useEffect(() => {
    const load = () => {
      api.getProjects().then((d) => setProjects(d.projects || [])).catch(() => {});
      api.getTasks({ status: "in_progress" }).then((d) => {
        const counts: Record<string, number> = {};
        for (const t of d.tasks || []) {
          counts[t.project] = (counts[t.project] || 0) + 1;
        }
        setActiveCounts(counts);
      }).catch(() => {});
    };
    load();
    const interval = setInterval(load, 15000);
    return () => clearInterval(interval);
  }, []);

  const selectedProject = channel ?? null;

  return (
    <div className="rail">
      {/* AEQI mark */}
      <div
        className={`rail-icon${!channel ? " active" : ""}`}
        onClick={() => { setChannel(null); navigate("/"); }}
        title="AEQI"
      >
        A
      </div>

      <div className="rail-separator" />

      {/* Project icons */}
      <div className="rail-projects">
        {projects.map((p) => {
          const isSelected = selectedProject === p.name;
          const hasActive = (activeCounts[p.name] || 0) > 0;

          return (
            <div key={p.name} className="rail-project-wrapper">
              {/* Selection pill */}
              <div className={`rail-pill${isSelected ? " rail-pill-selected" : ""}`} />

              <button
                className="rail-project-btn"
                onClick={() => { setChannel(p.name); navigate("/"); }}
                title={p.name}
              >
                <ProjectPatternIcon name={p.name} selected={isSelected} />

                {/* Live activity indicator */}
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
      </div>

      {/* Add project button */}
      <div className="rail-add" title="New project">+</div>

      {/* Bottom status */}
      <div className="rail-bottom">
        <div className={`rail-status-dot ${daemonOk ? "rail-status-ok" : "rail-status-err"}`} />
      </div>
    </div>
  );
}
