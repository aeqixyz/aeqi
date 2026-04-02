import { useEffect, useRef, useState } from "react";

// --- Subtle grid overlay ---
function GridOverlay() {
  return (
    <div className="pointer-events-none fixed inset-0 z-0 overflow-hidden">
      <div
        className="absolute inset-0"
        style={{
          backgroundImage: `
            linear-gradient(rgba(255,255,255,0.02) 1px, transparent 1px),
            linear-gradient(90deg, rgba(255,255,255,0.02) 1px, transparent 1px)
          `,
          backgroundSize: "80px 80px",
        }}
      />
    </div>
  );
}

// --- Fade-in on scroll ---
function FadeIn({
  children,
  className = "",
  delay = 0,
}: {
  children: React.ReactNode;
  className?: string;
  delay?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { threshold: 0.15 }
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      ref={ref}
      className={className}
      style={{
        opacity: visible ? 1 : 0,
        transform: visible ? "translateY(0)" : "translateY(16px)",
        transition: `opacity 0.8s ease ${delay}ms, transform 0.8s ease ${delay}ms`,
      }}
    >
      {children}
    </div>
  );
}

// --- System log feed ---
const LOG_ENTRIES = [
  { time: "00:00:01", event: "daemon started", detail: "patrol interval 30s" },
  { time: "00:00:02", event: "agent loaded", detail: "CTO (capable tier)" },
  { time: "00:00:02", event: "agent loaded", detail: "COO (balanced tier)" },
  {
    time: "00:00:03",
    event: "trigger fired",
    detail: "memory-consolidation",
  },
  { time: "00:00:04", event: "task created", detail: "sg-048 agent-bound" },
  { time: "00:00:05", event: "worker spawned", detail: "middleware: 9 layers" },
  {
    time: "00:00:12",
    event: "delegation",
    detail: "CTO → Backend via dispatch",
  },
  { time: "00:00:18", event: "task completed", detail: "cost: $0.042" },
  {
    time: "00:00:19",
    event: "memory stored",
    detail: "entity scope, key: arch-decision",
  },
  {
    time: "00:00:30",
    event: "patrol cycle",
    detail: "3 active, 0 pending",
  },
];

function SystemLog() {
  const [count, setCount] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setCount((c) => (c < LOG_ENTRIES.length ? c + 1 : c));
    }, 400);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="font-mono text-[13px] leading-relaxed">
      {LOG_ENTRIES.slice(0, count).map((entry, i) => (
        <div
          key={i}
          className="flex gap-4 py-1.5 border-b border-white/[0.04]"
          style={{
            animation: "fadeSlide 0.4s ease forwards",
            opacity: 0,
            animationDelay: `${i * 50}ms`,
          }}
        >
          <span className="text-white/20 shrink-0 w-16">{entry.time}</span>
          <span className="text-white/50 shrink-0 w-36">{entry.event}</span>
          <span className="text-white/30">{entry.detail}</span>
        </div>
      ))}
      {count >= LOG_ENTRIES.length && (
        <div className="flex gap-4 py-1.5 text-white/10">
          <span className="w-16">00:00:30</span>
          <span className="w-36 animate-pulse">waiting...</span>
          <span>next patrol in 30s</span>
        </div>
      )}
    </div>
  );
}

// --- System graph (SVG) ---
function SystemGraph() {
  const nodes = [
    { id: "agents", label: "Agents", x: 200, y: 80 },
    { id: "memory", label: "Memory", x: 400, y: 80 },
    { id: "triggers", label: "Triggers", x: 100, y: 220 },
    { id: "skills", label: "Skills", x: 300, y: 220 },
    { id: "delegation", label: "Delegation", x: 500, y: 220 },
    { id: "tasks", label: "Tasks", x: 200, y: 360 },
    { id: "middleware", label: "Middleware", x: 400, y: 360 },
  ];

  const edges = [
    ["agents", "memory"],
    ["agents", "skills"],
    ["agents", "delegation"],
    ["triggers", "agents"],
    ["triggers", "tasks"],
    ["skills", "tasks"],
    ["delegation", "tasks"],
    ["tasks", "middleware"],
    ["memory", "middleware"],
  ];

  const nodeMap = Object.fromEntries(nodes.map((n) => [n.id, n]));

  return (
    <svg viewBox="0 0 600 440" className="w-full max-w-[600px] mx-auto">
      {edges.map(([from, to], i) => {
        const a = nodeMap[from];
        const b = nodeMap[to];
        return (
          <line
            key={i}
            x1={a.x}
            y1={a.y}
            x2={b.x}
            y2={b.y}
            stroke="rgba(255,255,255,0.06)"
            strokeWidth="1"
          />
        );
      })}
      {nodes.map((node) => (
        <g key={node.id}>
          <circle
            cx={node.x}
            cy={node.y}
            r="6"
            fill="none"
            stroke="rgba(255,255,255,0.2)"
            strokeWidth="1"
          />
          <circle
            cx={node.x}
            cy={node.y}
            r="2"
            fill="rgba(255,255,255,0.4)"
          />
          <text
            x={node.x}
            y={node.y + 22}
            textAnchor="middle"
            fill="rgba(255,255,255,0.4)"
            fontSize="12"
            fontFamily="Inter, system-ui, sans-serif"
            fontWeight="400"
          >
            {node.label}
          </text>
        </g>
      ))}
    </svg>
  );
}

// --- Main ---
export default function App() {
  return (
    <div className="relative min-h-screen">
      <GridOverlay />
      <style>{`
        @keyframes fadeSlide {
          from { opacity: 0; transform: translateX(-8px); }
          to { opacity: 1; transform: translateX(0); }
        }
      `}</style>

      {/* Nav */}
      <nav className="relative z-10 flex items-center justify-between px-8 py-6 max-w-6xl mx-auto">
        <span className="text-[15px] font-medium tracking-[0.15em] text-white/80">
          AEQI
        </span>
        <div className="flex items-center gap-8 text-[13px] text-white/40">
          <a
            href="https://github.com/0xAEQI/aeqi"
            className="hover:text-white/70 transition-colors"
          >
            GitHub
          </a>
          <a
            href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md"
            className="hover:text-white/70 transition-colors"
          >
            Docs
          </a>
          <a
            href="https://app.aeqi.ai"
            className="px-4 py-1.5 border border-white/10 hover:border-white/20 hover:text-white/70 transition-colors"
          >
            Enter
          </a>
        </div>
      </nav>

      {/* Hero */}
      <section className="relative z-10 flex flex-col items-center justify-center min-h-[85vh] px-8 text-center">
        <FadeIn>
          <p className="text-[13px] tracking-[0.3em] uppercase text-white/25 mb-8">
            Persistent Agent Orchestration
          </p>
        </FadeIn>
        <FadeIn delay={150}>
          <h1 className="text-5xl sm:text-6xl md:text-7xl font-light tracking-tight text-white leading-[1.1] mb-6">
            Run a company
            <br />
            <span className="text-white/40">like software</span>
          </h1>
        </FadeIn>
        <FadeIn delay={300}>
          <p className="text-lg text-white/30 max-w-lg leading-relaxed mb-12">
            Agents that remember. Departments that coordinate.
            <br />
            Triggers that execute. One runtime.
          </p>
        </FadeIn>
        <FadeIn delay={450}>
          <div className="flex gap-4">
            <a
              href="https://app.aeqi.ai"
              className="px-6 py-3 text-[13px] font-medium tracking-wide border border-white/20 text-white/80 hover:bg-white/5 transition-colors"
            >
              Enter System
            </a>
            <a
              href="https://github.com/0xAEQI/aeqi"
              className="px-6 py-3 text-[13px] font-medium tracking-wide text-white/30 hover:text-white/50 transition-colors"
            >
              View Source
            </a>
          </div>
        </FadeIn>
        <div className="absolute bottom-12 left-1/2 -translate-x-1/2">
          <div className="w-px h-12 bg-gradient-to-b from-transparent to-white/10" />
        </div>
      </section>

      {/* What it is */}
      <section className="relative z-10 max-w-4xl mx-auto px-8 py-32">
        <FadeIn>
          <p className="text-[13px] tracking-[0.2em] uppercase text-white/20 mb-6">
            The Problem
          </p>
        </FadeIn>
        <FadeIn delay={100}>
          <h2 className="text-3xl sm:text-4xl font-light text-white/90 leading-snug mb-12">
            AI agents forget everything
            <br />
            <span className="text-white/30">
              between sessions
            </span>
          </h2>
        </FadeIn>
        <FadeIn delay={200}>
          <div className="grid md:grid-cols-2 gap-16 text-[15px] leading-relaxed">
            <div>
              <p className="text-white/20 text-[12px] tracking-[0.15em] uppercase mb-4">
                Typical agent frameworks
              </p>
              <ul className="space-y-3 text-white/30">
                <li>Stateless sessions, no continuity</li>
                <li>No coordination between agents</li>
                <li>No budget enforcement</li>
                <li>No safety middleware</li>
                <li>No organizational structure</li>
              </ul>
            </div>
            <div>
              <p className="text-white/20 text-[12px] tracking-[0.15em] uppercase mb-4">
                AEQI
              </p>
              <ul className="space-y-3 text-white/50">
                <li>Entity-scoped memory persists forever</li>
                <li>Delegation with 5 response modes</li>
                <li>Per-agent and per-project budgets</li>
                <li>9-layer middleware on every execution</li>
                <li>Department hierarchy with escalation</li>
              </ul>
            </div>
          </div>
        </FadeIn>
      </section>

      {/* Divider */}
      <div className="max-w-4xl mx-auto px-8">
        <div className="h-px bg-white/[0.05]" />
      </div>

      {/* System graph */}
      <section className="relative z-10 max-w-4xl mx-auto px-8 py-32">
        <FadeIn>
          <p className="text-[13px] tracking-[0.2em] uppercase text-white/20 mb-6">
            Architecture
          </p>
        </FadeIn>
        <FadeIn delay={100}>
          <h2 className="text-3xl sm:text-4xl font-light text-white/90 leading-snug mb-16">
            One system
          </h2>
        </FadeIn>
        <FadeIn delay={200}>
          <SystemGraph />
        </FadeIn>
        <FadeIn delay={300}>
          <p className="text-center text-[14px] text-white/20 mt-12 max-w-md mx-auto leading-relaxed">
            The orchestrator owns the runtime. Memory, safety, and coordination
            are not plugins -- they're built into every execution path.
          </p>
        </FadeIn>
      </section>

      {/* Divider */}
      <div className="max-w-4xl mx-auto px-8">
        <div className="h-px bg-white/[0.05]" />
      </div>

      {/* Live system */}
      <section className="relative z-10 max-w-4xl mx-auto px-8 py-32">
        <FadeIn>
          <p className="text-[13px] tracking-[0.2em] uppercase text-white/20 mb-6">
            Runtime
          </p>
        </FadeIn>
        <FadeIn delay={100}>
          <h2 className="text-3xl sm:text-4xl font-light text-white/90 leading-snug mb-12">
            Execution is autonomous
          </h2>
        </FadeIn>
        <FadeIn delay={200}>
          <div className="border border-white/[0.06] bg-white/[0.01] p-6 sm:p-8">
            <div className="flex items-center gap-3 mb-6 pb-4 border-b border-white/[0.04]">
              <div className="w-2 h-2 rounded-full bg-white/20" />
              <span className="text-[12px] text-white/20 font-mono tracking-wider">
                DAEMON LOG
              </span>
            </div>
            <SystemLog />
          </div>
        </FadeIn>
      </section>

      {/* Divider */}
      <div className="max-w-4xl mx-auto px-8">
        <div className="h-px bg-white/[0.05]" />
      </div>

      {/* Primitives */}
      <section className="relative z-10 max-w-4xl mx-auto px-8 py-32">
        <FadeIn>
          <p className="text-[13px] tracking-[0.2em] uppercase text-white/20 mb-6">
            Primitives
          </p>
        </FadeIn>
        <FadeIn delay={100}>
          <h2 className="text-3xl sm:text-4xl font-light text-white/90 leading-snug mb-16">
            Four concepts, nothing else
          </h2>
        </FadeIn>
        <div className="grid sm:grid-cols-2 gap-12">
          {[
            {
              name: "Agent",
              desc: "Persistent identity with UUID, entity memory, department membership. Not a process -- loaded on demand, knowledge persists.",
            },
            {
              name: "Department",
              desc: "Organizational hierarchy. Controls escalation, blackboard visibility, and clarification routing.",
            },
            {
              name: "Task",
              desc: "Always agent-bound. Atomic checkout, validated state transitions, adaptive retry with failure analysis.",
            },
            {
              name: "Delegation",
              desc: "One tool for all interaction. Named agents, departments, subagents. Five response routing modes.",
            },
          ].map((p, i) => (
            <FadeIn key={p.name} delay={150 + i * 100}>
              <div>
                <h3 className="text-[14px] font-medium text-white/60 mb-3 tracking-wide">
                  {p.name}
                </h3>
                <p className="text-[14px] text-white/25 leading-relaxed">
                  {p.desc}
                </p>
              </div>
            </FadeIn>
          ))}
        </div>
      </section>

      {/* Divider */}
      <div className="max-w-4xl mx-auto px-8">
        <div className="h-px bg-white/[0.05]" />
      </div>

      {/* Trust / infrastructure */}
      <section className="relative z-10 max-w-4xl mx-auto px-8 py-32 text-center">
        <FadeIn>
          <p className="text-[13px] tracking-[0.2em] uppercase text-white/20 mb-8">
            Infrastructure
          </p>
        </FadeIn>
        <FadeIn delay={150}>
          <h2 className="text-3xl sm:text-4xl font-light text-white/90 leading-snug mb-8">
            Rust. SQLite. 634 tests.
          </h2>
        </FadeIn>
        <FadeIn delay={300}>
          <p className="text-[15px] text-white/25 max-w-lg mx-auto leading-relaxed mb-4">
            10 crates. 9 middleware layers. Zero unsafe. MIT licensed.
          </p>
        </FadeIn>
        <FadeIn delay={400}>
          <div className="inline-flex gap-6 mt-8 text-[13px] font-mono text-white/15">
            <span>aeqi-core</span>
            <span>aeqi-orchestrator</span>
            <span>aeqi-memory</span>
            <span>aeqi-web</span>
          </div>
        </FadeIn>
      </section>

      {/* Footer */}
      <footer className="relative z-10 max-w-4xl mx-auto px-8 py-16 flex items-center justify-between border-t border-white/[0.04]">
        <span className="text-[13px] text-white/15">
          aeqi.ai
        </span>
        <div className="flex gap-6 text-[13px] text-white/15">
          <a
            href="https://github.com/0xAEQI/aeqi"
            className="hover:text-white/30 transition-colors"
          >
            GitHub
          </a>
          <a
            href="https://github.com/0xAEQI/aeqi/blob/main/LICENSE"
            className="hover:text-white/30 transition-colors"
          >
            MIT License
          </a>
        </div>
      </footer>
    </div>
  );
}
