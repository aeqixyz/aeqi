import { useState, lazy, Suspense, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";

const ParticleLogo = lazy(() => import("./ParticleLogo"));

/* ─── Animation helpers ─── */
const fade = (delay = 0) => ({
  initial: { opacity: 0, y: 8 } as const,
  animate: { opacity: 1, y: 0 } as const,
  transition: { duration: 0.7, ease: [0.25, 0.1, 0.25, 1] as const, delay },
});

const fadeView = (delay = 0) => ({
  initial: { opacity: 0, y: 16 } as const,
  whileInView: { opacity: 1, y: 0 } as const,
  viewport: { once: true, margin: "-40px" } as const,
  transition: { duration: 0.7, ease: [0.25, 0.1, 0.25, 1] as const, delay },
});

/* ─── Nav ─── */
function Nav() {
  return (
    <motion.nav
      className="fixed top-0 left-0 right-0 z-50 flex justify-center pt-4 px-4"
      initial={{ opacity: 0, y: -20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.6, delay: 0.15, ease: [0.25, 0.1, 0.25, 1] }}
    >
      <div className="w-full max-w-3xl backdrop-blur-2xl bg-white/60 border border-black/[0.06] rounded-2xl shadow-lg shadow-black/[0.03] px-5 h-12 flex items-center justify-between">
        <a href="/" className="text-[16px] font-semibold tracking-tight text-black/60 hover:text-black/80 transition-colors">
          aeqi
        </a>
        <div className="flex items-center gap-1">
          <a href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md" className="text-[13px] text-black/40 hover:text-black/70 hover:bg-black/[0.04] rounded-lg px-3 py-1.5 transition-all hidden md:block">
            Docs
          </a>
          <a href="https://github.com/0xAEQI/aeqi" className="text-[13px] text-black/40 hover:text-black/70 hover:bg-black/[0.04] rounded-lg px-3 py-1.5 transition-all">
            GitHub
          </a>
          <a href="https://aeqi.ai/enterprise" className="text-[13px] text-black/40 hover:text-black/70 hover:bg-black/[0.04] rounded-lg px-3 py-1.5 transition-all hidden md:block">
            Enterprise
          </a>
          <div className="w-px h-5 bg-black/[0.08] mx-1.5" />
          <a
            href="https://app.aeqi.ai"
            className="bg-black text-white rounded-xl px-4 py-1.5 text-[13px] font-medium hover:bg-black/85 transition-all hover:shadow-md hover:shadow-black/10 active:scale-[0.97]"
          >
            Get Started
          </a>
        </div>
      </div>
    </motion.nav>
  );
}

/* ─── Hero ─── */
function Hero() {
  const [copied, setCopied] = useState(false);
  const [showParticles, setShowParticles] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setShowParticles(true), 800);
    return () => clearTimeout(timer);
  }, []);

  const copy = () => {
    navigator.clipboard.writeText("cargo install aeqi");
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <section className="flex-1 flex items-center justify-center px-6 min-h-[80vh]">
      <div className="max-w-2xl mx-auto text-center">
        {/* Logo */}
        <motion.div {...fade(0.1)} className="flex justify-center" style={{ height: 280 }}>
          <AnimatePresence mode="wait">
            {!showParticles ? (
              <motion.span
                key="solid"
                className="text-[160px] md:text-[200px] font-bold tracking-tighter leading-none text-black/50 select-none"
                style={{ lineHeight: "280px" }}
                exit={{ opacity: 0, scale: 1.02 }}
                transition={{ duration: 0.25 }}
              >
                æ
              </motion.span>
            ) : (
              <motion.div
                key="particles"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ duration: 0.15 }}
              >
                <Suspense fallback={null}>
                  <ParticleLogo size={280} />
                </Suspense>
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>

        {/* Headline */}
        <motion.h1
          className="mt-2 text-[22px] md:text-[28px] font-semibold tracking-tight text-black/80 leading-snug"
          {...fade(0.3)}
        >
          Unlock the agent economy.
          <br />
          <span className="text-black/40">Companies that run, learn, and fund themselves.</span>
        </motion.h1>

        {/* CTA */}
        <motion.div className="mt-10 flex flex-col items-center gap-4" {...fade(0.45)}>
          <a
            href="https://app.aeqi.ai"
            className="inline-block bg-black text-white rounded-full px-8 py-3 text-[15px] font-medium hover:bg-black/80 transition-all hover:shadow-xl hover:shadow-black/10 hover:scale-[1.02] active:scale-[0.98]"
          >
            Launch a Company
          </a>
          <button
            onClick={copy}
            className="group inline-flex items-center gap-2.5 text-[13px] text-black/30 hover:text-black/50 transition-colors cursor-pointer"
          >
            <code className="font-mono">
              <span className="select-none opacity-50">$ </span>
              cargo install aeqi
            </code>
            <span className="text-[11px] opacity-60 group-hover:opacity-100 transition-opacity">
              {copied ? "✓" : "copy"}
            </span>
          </button>
        </motion.div>
      </div>
    </section>
  );
}

/* ─── Value props ─── */
const props = [
  { title: "Agent orchestration", desc: "Agents delegate, coordinate, and execute across every function — engineering, marketing, operations, finance. The company runs itself. You set direction, not tasks." },
  { title: "Autonomous compounding", desc: "Every task teaches. Every failure refines. Agents learn from everything they do, find new opportunities, and evolve to maximize shareholder value — automatically." },
  { title: "Optional instant capital formation", desc: "The cap table is a smart contract. Equity is tokenized from day one. Raising capital is a transaction, not a quarter of legal fees and term sheets." },
];

function ValueProps() {
  return (
    <section className="py-24 px-6">
      <div className="max-w-5xl mx-auto">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-12 md:gap-16">
          {props.map((p, i) => (
            <motion.div key={p.title} {...fadeView(0.1 * i)}>
              <h3 className="text-[15px] font-semibold text-black/70 mb-2">{p.title}</h3>
              <p className="text-[14px] leading-relaxed text-black/35">{p.desc}</p>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}

/* ─── Closing CTA ─── */
function ClosingCTA() {
  return (
    <section className="py-20 px-6">
      <motion.div className="max-w-xl mx-auto text-center" {...fadeView()}>
        <p className="text-[13px] uppercase tracking-[0.2em] text-black/20 mb-4">
          Get started
        </p>
        <h2 className="text-[24px] md:text-[32px] font-semibold tracking-tight text-black/70 leading-snug">
          Start something that runs itself.
        </h2>
        <div className="mt-8 flex flex-col sm:flex-row items-center justify-center gap-4">
          <a
            href="https://app.aeqi.ai"
            className="inline-block bg-black text-white rounded-full px-8 py-3 text-[15px] font-medium hover:bg-black/80 transition-all hover:shadow-xl hover:shadow-black/10 hover:scale-[1.02] active:scale-[0.98]"
          >
            Launch a Company
          </a>
          <a
            href="https://aeqi.ai/enterprise"
            className="inline-block text-[14px] text-black/40 hover:text-black/60 transition-colors"
          >
            Talk to us →
          </a>
        </div>
      </motion.div>
    </section>
  );
}

/* ─── Footer ─── */
function Footer() {
  return (
    <footer className="border-t border-black/[0.04]">
      <div className="max-w-5xl mx-auto px-6 py-14 w-full">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-10">
          <motion.div {...fadeView(0.05)}>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/20 mb-4">Product</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://app.aeqi.ai" className="block text-black/35 hover:text-black/60 transition-colors">Launch a Company</a>
              <a href="https://aeqi.ai/enterprise" className="block text-black/35 hover:text-black/60 transition-colors">Enterprise</a>
              <a href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md" className="block text-black/35 hover:text-black/60 transition-colors">Docs</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.1)}>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/20 mb-4">Community</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://github.com/0xAEQI/aeqi" className="block text-black/35 hover:text-black/60 transition-colors">GitHub</a>
              <a href="https://x.com/0xAEQI" className="block text-black/35 hover:text-black/60 transition-colors">X</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.15)}>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/20 mb-4">Legal</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://aeqi.ai/terms" className="block text-black/35 hover:text-black/60 transition-colors">Terms</a>
              <a href="https://aeqi.ai/privacy" className="block text-black/35 hover:text-black/60 transition-colors">Privacy</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.2)} className="flex flex-col justify-between">
            <div>
              <span className="text-[28px] font-bold tracking-tighter text-black/40 leading-none">æ</span>
              <p className="mt-3 text-[12px] text-black/15">
                &copy; {new Date().getFullYear()} aeqi.ai
              </p>
            </div>
          </motion.div>
        </div>
      </div>
    </footer>
  );
}

/* ─── App ─── */
export default function App() {
  return (
    <div className="min-h-screen flex flex-col bg-white">
      <Nav />
      <Hero />
      <div className="bg-[#fafafa]">
        <ValueProps />
        <ClosingCTA />
        <Footer />
      </div>
    </div>
  );
}
