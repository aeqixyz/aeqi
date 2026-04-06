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
    <section className="flex-1 flex items-center justify-center px-6 min-h-[70vh] pt-20">
      <div className="max-w-2xl mx-auto text-center">
        {/* Logo — "i" drops, locks, burst */}
        <div className="flex justify-center" style={{ height: 160, position: "relative" }}>
          <AnimatePresence mode="wait">
            {!showParticles ? (
              <motion.div
                key="solid"
                className="flex items-center justify-center select-none"
                style={{ height: 160 }}
                exit={{ opacity: 0, scale: 1.08 }}
                transition={{ duration: 0.2 }}
              >
                <motion.span
                  className="text-[90px] md:text-[110px] font-bold tracking-[-0.08em] leading-none text-black/50"
                  style={{ lineHeight: "160px" }}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 0.25 }}
                >
                  æq
                </motion.span>
                <motion.span
                  className="text-[90px] md:text-[110px] font-bold tracking-[-0.08em] leading-none text-black/50 inline-block"
                  style={{ lineHeight: "160px", translateY: "0.04em" }}
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
                  <ParticleLogo width={400} height={160} />
                </Suspense>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* Headline */}
        <h1 className="text-[24px] md:text-[32px] font-semibold tracking-tight text-black/85 leading-snug">
          Unlock the agent economy.
          <br />
          <motion.span
            className="text-black/50"
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: showParticles ? 1 : 0, y: showParticles ? 0 : 4 }}
            transition={{ duration: 0.5, ease: [0.25, 0.1, 0.25, 1] }}
          >
            AI agents that run your company, grow, and fund themselves.
          </motion.span>
        </h1>

        {/* CTAs */}
        <div className="mt-8 flex flex-col sm:flex-row items-center justify-center gap-3">
          <a
            href="https://app.aeqi.ai/signup"
            className="inline-block bg-black text-white rounded-full px-8 py-3 text-[15px] font-medium hover:bg-black/80 transition-all hover:shadow-xl hover:shadow-black/10 hover:scale-[1.02] active:scale-[0.98]"
          >
            Launch a Company
          </a>
          <a
            href="https://github.com/0xAEQI/aeqi"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 border border-black/10 text-black/60 rounded-full px-6 py-3 text-[14px] font-medium hover:border-black/20 hover:text-black/80 transition-all"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" /></svg>
            Run it yourself
          </a>
        </div>

        <p className="mt-4 text-[12px] text-black/25">7-day free trial · No credit card · Plans from $29/mo</p>

        {/* Scroll arrow */}
        <motion.div
          className="mt-12"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 2, duration: 0.8 }}
        >
          <motion.svg
            width="20" height="20" viewBox="0 0 20 20" fill="none"
            stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"
            className="text-black/15 mx-auto"
            animate={{ y: [0, 6, 0] }}
            transition={{ duration: 2, repeat: Infinity, ease: "easeInOut" }}
          >
            <path d="M4 7l6 6 6-6" />
          </motion.svg>
        </motion.div>
      </div>
    </section>
  );
}

/* ─── How it works ─── */
function HowItWorks() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-4xl mx-auto">
        <motion.div {...fadeView()} className="text-center mb-16">
          <h2 className="text-[24px] md:text-[30px] font-semibold tracking-tight text-black/85 leading-snug">
            Launch a company that never sleeps.
          </h2>
          <p className="text-[15px] text-black/40 mt-3">
            Set the mission. Hire agents. Watch it run.
          </p>
        </motion.div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-10 md:gap-14">
          <motion.div {...fadeView(0)}>
            <h3 className="text-[17px] font-semibold text-black/85 mb-3">Agents, not employees.</h3>
            <p className="text-[15px] leading-relaxed text-black/50">
              An entire company, staffed by agents. Engineering, growth, operations, finance — coordinating in real time. No employees. No overhead. You set the mission. They run the company.
            </p>
          </motion.div>

          <motion.div {...fadeView(0.1)}>
            <h3 className="text-[17px] font-semibold text-black/85 mb-3">Memory that compounds.</h3>
            <p className="text-[15px] leading-relaxed text-black/50">
              Every session, every outcome, every decision gets stored. Agents remember everything, learn from every outcome, and find new edges on their own. The longer it runs, the more it's worth.
            </p>
          </motion.div>

          <motion.div {...fadeView(0.2)}>
            <h3 className="text-[17px] font-semibold text-black/85 mb-3">Instant capital formation.</h3>
            <p className="text-[15px] leading-relaxed text-black/50">
              Equity is tokenized from day one. Investors fund a company in one transaction. No term sheets, no board seats, no waiting. Revenue and burn, visible on-chain in real time.
            </p>
          </motion.div>
        </div>

        <motion.div {...fadeView(0.3)} className="text-center mt-14 flex flex-col items-center gap-4">
          <a
            href="https://app.aeqi.ai/signup"
            className="inline-block bg-black text-white rounded-full px-8 py-3 text-[15px] font-medium hover:bg-black/80 transition-all hover:shadow-xl hover:shadow-black/10 hover:scale-[1.02] active:scale-[0.98]"
          >
            Start a free trial
          </a>
          <a
            href="/pricing"
            className="text-[14px] text-black/40 hover:text-black/60 transition-colors underline underline-offset-4 decoration-black/15 hover:decoration-black/30"
          >
            View pricing →
          </a>
        </motion.div>
      </div>
    </section>
  );
}

/* ─── Built in the open ─── */
function BuiltInTheOpen() {
  const [copied, setCopied] = useState(false);

  return (
    <section className="py-24 px-6">
      <div className="max-w-xl mx-auto text-center">
        <motion.div {...fadeView()}>
          <p className="text-[11px] font-medium tracking-[0.2em] uppercase text-black/20 mb-4">Source Available</p>
          <h2 className="text-[24px] md:text-[30px] font-semibold tracking-tight text-black/85 leading-snug mb-4">
            Built in the open.
          </h2>
          <p className="text-[15px] text-black/40 mb-10">
            Run it yourself. Own your infrastructure.
          </p>
        </motion.div>

        <motion.div {...fadeView(0.1)} className="flex flex-col items-center gap-0 mb-10 w-full max-w-md mx-auto">
          <a
            href="https://github.com/0xAEQI/aeqi"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-4 bg-[#fafafa] border border-black/[0.06] rounded-t-2xl px-6 py-4 hover:bg-black/[0.03] transition-all group w-full"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" className="text-black/60 flex-shrink-0">
              <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
            </svg>
            <span className="flex-1 text-[14px] font-medium text-black/70 group-hover:text-black/90 transition-colors text-left">Star on GitHub</span>
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" className="text-black/15 group-hover:text-black/40 transition-colors">
              <path d="M6 3l5 5-5 5" />
            </svg>
          </a>

          <div className="flex items-center gap-3 w-full px-6 py-2">
            <div className="flex-1 h-px bg-black/[0.06]" />
            <span className="text-[11px] text-black/20 uppercase tracking-widest">or</span>
            <div className="flex-1 h-px bg-black/[0.06]" />
          </div>

          <button
            onClick={() => { navigator.clipboard.writeText("curl -fsSL https://raw.githubusercontent.com/0xAEQI/aeqi/main/scripts/install.sh | sh"); setCopied(true); setTimeout(() => setCopied(false), 2000); }}
            className="group flex items-center gap-4 bg-[#fafafa] border border-black/[0.06] rounded-b-2xl px-6 py-4 hover:bg-black/[0.03] transition-all cursor-pointer w-full"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" className="text-black/40 flex-shrink-0">
              <path d="M4 17l6-6-6-6M12 19h8" />
            </svg>
            <code className="flex-1 font-mono text-[13px] font-medium text-black/50 group-hover:text-black/70 transition-colors text-left">
              curl -fsSL https://aeqi.ai/install.sh | sh
            </code>
            <span className="text-[11px] text-black/20 group-hover:text-black/40 transition-colors">
              {copied ? "✓" : "copy"}
            </span>
          </button>
        </motion.div>
      </div>
    </section>
  );
}

/* ─── App ─── */
export default function App() {
  return (
    <div className="min-h-screen flex flex-col bg-white">
      <Nav />
      <Hero />
      <div className="bg-[#fafafa]">
        <HowItWorks />
      </div>
      <div className="bg-white">
        <BuiltInTheOpen />
      </div>
      <div className="bg-[#fafafa]">
        <Footer />
      </div>
    </div>
  );
}
