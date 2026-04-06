import { useState, lazy, Suspense, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import Nav from "./Nav";
import Footer from "./Footer";

const ParticleLogo = lazy(() => import("./ParticleLogo"));

/* ─── Animation helpers ─── */
const fadeView = (delay = 0) => ({
  initial: { opacity: 0, y: 16 } as const,
  whileInView: { opacity: 1, y: 0 } as const,
  viewport: { once: true, margin: "-40px" } as const,
  transition: { duration: 0.7, ease: [0.25, 0.1, 0.25, 1] as const, delay },
});

/* ─── Hero ─── */
function Hero() {
  const [showParticles, setShowParticles] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setShowParticles(true), 700);
    return () => clearTimeout(timer);
  }, []);

  return (
    <section className="flex-1 flex items-center justify-center px-6 min-h-[80vh]">
      <div className="max-w-2xl mx-auto text-center">
        {/* Logo — "i" drops from top, locks in, then particle burst */}
        <div className="flex justify-center" style={{ height: 200, position: "relative" }}>
          <AnimatePresence mode="wait">
            {!showParticles ? (
              <motion.div
                key="solid"
                className="flex items-center justify-center select-none"
                style={{ height: 200 }}
                exit={{ opacity: 0, scale: 1.08 }}
                transition={{ duration: 0.2 }}
              >
                {/* "æq" appears instantly */}
                <motion.span
                  className="text-[110px] md:text-[140px] font-bold tracking-[-0.08em] leading-none text-black/50"
                  style={{ lineHeight: "200px" }}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 0.25 }}
                >
                  æq
                </motion.span>
                {/* "i" drops fast from top */}
                <motion.span
                  className="text-[110px] md:text-[140px] font-bold tracking-[-0.08em] leading-none text-black/50 inline-block"
                  style={{ lineHeight: "200px", translateY: "0.04em" }}
                  initial={{ y: "-50vh", opacity: 0 }}
                  animate={{ y: 0, opacity: 1 }}
                  transition={{
                    y: { duration: 0.4, delay: 0.1, ease: [0.22, 1, 0.36, 1] },
                    opacity: { duration: 0.15, delay: 0.1 },
                  }}
                >
                  i
                </motion.span>
              </motion.div>
            ) : (
              <motion.div
                key="particles"
                initial={{ opacity: 0, scale: 0.95 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ duration: 0.4, ease: [0.25, 0.1, 0.25, 1] }}
              >
                <Suspense fallback={null}>
                  <ParticleLogo width={500} height={200} />
                </Suspense>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* Headline — always visible, subtitle fades in with burst */}
        <h1 className="mt-2 text-[26px] md:text-[34px] font-semibold tracking-tight text-black/85 leading-snug">
          Unlock the agent economy.
          <br />
          <motion.span
            className="text-black/60"
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: showParticles ? 1 : 0, y: showParticles ? 0 : 4 }}
            transition={{ duration: 0.5, ease: [0.25, 0.1, 0.25, 1] }}
          >
            Companies that run, learn, and fund themselves.
          </motion.span>
        </h1>

        {/* CTA — always visible */}
        <div className="mt-10">
          <a
            href="https://app.aeqi.ai/signup"
            className="inline-block bg-black text-white rounded-full px-8 py-3.5 text-[15px] font-medium hover:bg-black/80 transition-all hover:shadow-xl hover:shadow-black/10 hover:scale-[1.02] active:scale-[0.98]"
          >
            Launch a Company
          </a>
        </div>
      </div>
    </section>
  );
}

/* ─── Value props ─── */
const props = [
  { title: "Agent orchestration", desc: "An entire company, staffed by agents. Engineering, growth, operations, finance. Coordinating in real time. No employees. No overhead. You set the mission. They run the company." },
  { title: "Autonomous compounding", desc: "The company gets smarter every day. Agents remember everything, learn from every outcome, and find new edges on their own. The longer it runs, the more it's worth." },
  { title: "Instant capital formation", desc: "Equity is tokenized from day one. Investors fund a company in one transaction. No term sheets, no board seats, no waiting. Revenue and burn, visible on-chain in real time." },
];

function ValueProps() {
  return (
    <section className="py-32 px-6">
      <div className="max-w-4xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-10 md:gap-14">
          {props.map((p, i) => (
            <motion.div key={p.title} {...fadeView(0.1 * i)}>
              <h3 className="text-[17px] font-semibold text-black/85 mb-3">{p.title}</h3>
              <p className="text-[16px] leading-relaxed text-black/65">{p.desc}</p>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}

/* ─── Open Source ─── */
function OpenSource() {
  const [copied, setCopied] = useState(false);

  return (
    <section className="py-24 px-6">
      <div className="max-w-3xl mx-auto">
        <motion.div {...fadeView()} className="text-center mb-16">
          <p className="text-[11px] font-medium tracking-[0.2em] uppercase text-black/20 mb-4">Open Source</p>
          <h2 className="text-[24px] md:text-[30px] font-semibold tracking-tight text-black/85 leading-snug">
            Built in the open.<br />
            <span className="text-black/50">Run it yourself. Own your infrastructure.</span>
          </h2>
        </motion.div>

        <motion.div {...fadeView(0.1)} className="grid grid-cols-1 md:grid-cols-2 gap-8 mb-14">
          {/* Architecture */}
          <div className="bg-white rounded-2xl border border-black/[0.06] p-6">
            <pre className="text-[12px] font-mono text-black/50 leading-relaxed whitespace-pre overflow-x-auto">
{`aeqi (single binary, ~24MB)
├── daemon     orchestration, workers, patrol
├── web        REST API + WebSocket + dashboard
├── sqlite     agents, tasks, memory, sessions
└── embedded   React dashboard (rust-embed)

$ aeqi setup   # configure provider
$ aeqi start   # everything on :8400`}
            </pre>
          </div>

          {/* Stats */}
          <div className="flex flex-col gap-4">
            <a
              href="https://github.com/0xAEQI/aeqi"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-4 bg-white rounded-2xl border border-black/[0.06] p-5 hover:border-black/15 transition-all group"
            >
              <svg width="28" height="28" viewBox="0 0 24 24" fill="currentColor" className="text-black/70 flex-shrink-0">
                <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
              </svg>
              <div className="flex-1">
                <span className="text-[14px] font-medium text-black/80 group-hover:text-black transition-colors">0xAEQI/aeqi</span>
                <p className="text-[12px] text-black/40 mt-0.5">10 Rust crates · 600+ tests · BSL 1.1</p>
              </div>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" className="text-black/20 group-hover:text-black/50 transition-colors">
                <path d="M6 3l5 5-5 5" />
              </svg>
            </a>

            <div className="grid grid-cols-3 gap-3">
              <div className="bg-white rounded-xl border border-black/[0.06] p-4 text-center">
                <div className="text-[20px] font-bold text-black/80 font-mono">10</div>
                <div className="text-[11px] text-black/30 mt-1">crates</div>
              </div>
              <div className="bg-white rounded-xl border border-black/[0.06] p-4 text-center">
                <div className="text-[20px] font-bold text-black/80 font-mono">1</div>
                <div className="text-[11px] text-black/30 mt-1">binary</div>
              </div>
              <div className="bg-white rounded-xl border border-black/[0.06] p-4 text-center">
                <div className="text-[20px] font-bold text-black/80 font-mono">0</div>
                <div className="text-[11px] text-black/30 mt-1">dependencies*</div>
              </div>
            </div>
            <p className="text-[11px] text-black/20 text-center">*No Docker, no Postgres, no Redis. Just the binary.</p>
          </div>
        </motion.div>

        {/* Install commands */}
        <motion.div {...fadeView(0.2)} className="flex flex-col items-center gap-3">
          <button
            onClick={() => { navigator.clipboard.writeText("curl -fsSL https://raw.githubusercontent.com/0xAEQI/aeqi/main/scripts/install.sh | sh"); setCopied(true); setTimeout(() => setCopied(false), 2000); }}
            className="group inline-flex items-center gap-3 bg-white border border-black/[0.08] hover:border-black/15 rounded-xl px-5 py-3 text-[13px] text-black/60 hover:text-black/80 transition-all cursor-pointer"
          >
            <code className="font-mono font-medium">
              <span className="select-none text-black/25">$ </span>
              curl -fsSL https://aeqi.ai/install.sh | sh
            </code>
            <span className="text-[11px] text-black/30 group-hover:text-black/50 transition-colors">
              {copied ? "✓ copied" : "copy"}
            </span>
          </button>
          <button
            onClick={() => { navigator.clipboard.writeText("cargo install aeqi"); setCopied(true); setTimeout(() => setCopied(false), 2000); }}
            className="group inline-flex items-center gap-3 bg-white border border-black/[0.08] hover:border-black/15 rounded-xl px-5 py-2.5 text-[13px] text-black/40 hover:text-black/60 transition-all cursor-pointer"
          >
            <code className="font-mono font-medium">
              <span className="select-none text-black/20">$ </span>
              cargo install aeqi
            </code>
            <span className="text-[11px] text-black/20 group-hover:text-black/40 transition-colors">
              {copied ? "✓ copied" : "copy"}
            </span>
          </button>
          <p className="text-[11px] text-black/20 mt-1">Linux, macOS, WSL · No Docker required</p>
        </motion.div>
      </div>
    </section>
  );
}

/* ─── Section heading (above value props) ─── */
function SectionHeading() {
  return (
    <motion.div className="text-center pt-24 pb-8 px-6" {...fadeView()}>
      <h2 className="text-[26px] md:text-[32px] font-semibold tracking-tight text-black/85 leading-snug">
        Launch a company<br />that never sleeps.
      </h2>
      <p className="text-[15px] text-black/40 mt-4">
        Set the mission. Hire agents. Watch it run.
      </p>
      <div className="mt-6">
        <a
          href="/pricing"
          className="text-[14px] text-black/40 hover:text-black/60 transition-colors"
        >
          View pricing →
        </a>
      </div>
    </motion.div>
  );
}

/* ─── App ─── */
export default function App() {
  return (
    <div className="min-h-screen flex flex-col bg-white">
      <Nav />
      <Hero />
      <div className="bg-[#fafafa]">
        <SectionHeading />
        <ValueProps />
      </div>
      <div className="bg-white">
        <OpenSource />
      </div>
      <div className="bg-[#fafafa]">
        <Footer />
      </div>
    </div>
  );
}
