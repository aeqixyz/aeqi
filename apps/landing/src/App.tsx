import { useState, lazy, Suspense, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import Nav from "./Nav";
import Footer from "./Footer";

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
        <motion.div {...fade(0.1)} className="flex justify-center" style={{ height: 200 }}>
          <AnimatePresence mode="wait">
            {!showParticles ? (
              <motion.span
                key="solid"
                className="text-[110px] md:text-[140px] font-bold tracking-tighter leading-none text-black/50 select-none"
                style={{ lineHeight: "200px" }}
                exit={{ opacity: 0, scale: 1.02 }}
                transition={{ duration: 0.25 }}
              >
                æqi
              </motion.span>
            ) : (
              <motion.div
                key="particles"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ duration: 0.15 }}
              >
                <Suspense fallback={null}>
                  <ParticleLogo width={500} height={200} />
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
          <span className="text-black/55">Companies that run, learn, and fund themselves.</span>
        </motion.h1>

        {/* CTA */}
        <motion.div className="mt-10 flex flex-col items-center gap-4" {...fade(0.45)}>
          <a
            href="https://app.aeqi.ai/signup"
            className="inline-block bg-black text-white rounded-full px-8 py-3 text-[15px] font-medium hover:bg-black/80 transition-all hover:shadow-xl hover:shadow-black/10 hover:scale-[1.02] active:scale-[0.98]"
          >
            Launch a Company
          </a>
          <button
            onClick={copy}
            className="group inline-flex items-center gap-2 text-[13px] text-black/40 hover:text-black/60 transition-colors cursor-pointer"
          >
            <span className="text-black/35">or</span>
            <code className="font-mono">
              <span className="select-none opacity-50">$ </span>
              cargo install aeqi
            </code>
            <span className="text-[11px] opacity-0 group-hover:opacity-100 transition-opacity">
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
              <h3 className="text-[17px] font-semibold text-black/80 mb-3">{p.title}</h3>
              <p className="text-[15px] leading-[1.7] text-black/60">{p.desc}</p>
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
    <section className="pt-8 pb-24 px-6">
      <motion.div className="max-w-xl mx-auto text-center" {...fadeView()}>
        <h2 className="text-[20px] md:text-[24px] font-semibold tracking-tight text-black/80 leading-snug">
          Launch a company that never sleeps.
        </h2>
        <div className="mt-6">
          <a
            href="/pricing"
            className="inline-block text-[14px] text-black/50 hover:text-black/70 transition-colors"
          >
            View pricing →
          </a>
        </div>
      </motion.div>
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
        <ValueProps />
        <ClosingCTA />
        <Footer />
      </div>
    </div>
  );
}
