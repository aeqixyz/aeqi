import { useState } from "react";
import { motion } from "framer-motion";

/* ─── Fade-in helper ─── */
const fade = (delay = 0) => ({
  initial: { opacity: 0, y: 8 } as const,
  animate: { opacity: 1, y: 0 } as const,
  transition: { duration: 0.5, ease: "easeOut" as const, delay },
});

const fadeView = (delay = 0) => ({
  initial: { opacity: 0, y: 12 } as const,
  whileInView: { opacity: 1, y: 0 } as const,
  viewport: { once: true, margin: "-60px" } as const,
  transition: { duration: 0.5, ease: "easeOut" as const, delay },
});

/* ─── Nav ─── */
function Nav() {
  return (
    <motion.nav
      className="fixed top-0 left-0 right-0 z-50 backdrop-blur-lg bg-white/80 border-b border-black/5"
      {...fade(0.1)}
    >
      <div className="max-w-5xl mx-auto px-6 h-14 flex items-center justify-between">
        <a href="/" className="text-[20px] font-bold tracking-tight text-black">
          aeqi
        </a>
        <div className="flex items-center gap-5">
          <a
            href="https://github.com/0xAEQI/aeqi"
            className="text-[14px] text-black/40 hover:text-black/70 transition-colors"
          >
            github
          </a>
          <a
            href="https://aeqi.ai/enterprise"
            className="text-[14px] text-black/40 hover:text-black/70 transition-colors"
          >
            enterprise
          </a>
          <a
            href="https://app.aeqi.ai"
            className="bg-black text-white rounded-full px-5 py-2 text-[14px] font-medium hover:bg-black/85 transition-colors"
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

  const copy = () => {
    navigator.clipboard.writeText("cargo install aeqi");
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <section className="flex-1 flex items-center justify-center px-6">
      <div className="max-w-3xl mx-auto text-center py-32">
        <motion.div {...fade(0.1)}>
          <span className="text-[120px] md:text-[180px] font-bold tracking-tighter leading-none text-black select-none">
            æ
          </span>
        </motion.div>

        <motion.p
          className="mt-5 text-lg md:text-xl text-black/40 tracking-wide"
          {...fade(0.35)}
        >
          Companies run by agents. Ownership is programmable.
        </motion.p>

        <motion.div className="mt-10" {...fade(0.5)}>
          <button
            onClick={copy}
            className="group inline-flex items-center gap-3 bg-black/[0.03] hover:bg-black/[0.06] rounded-lg px-5 py-3 transition-colors cursor-pointer"
          >
            <code className="text-[14px] font-mono text-black/50">
              <span className="text-black/25 select-none">$&nbsp;</span>
              cargo install aeqi
            </code>
            <span className="text-[12px] text-black/20 group-hover:text-black/40 transition-colors">
              {copied ? "copied" : "copy"}
            </span>
          </button>
        </motion.div>
      </div>
    </section>
  );
}


/* ─── Footer ─── */
function Footer() {
  return (
    <footer className="flex flex-col">
      {/* Links */}
      <div className="max-w-5xl mx-auto px-6 py-12 w-full">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-8">
          <motion.div {...fadeView(0.05)}>
            <p className="text-[11px] uppercase tracking-[0.25em] text-black/20 mb-4">Product</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://app.aeqi.ai" className="block text-black/40 hover:text-black transition-colors">Get Started</a>
              <a href="https://aeqi.ai/enterprise" className="block text-black/40 hover:text-black transition-colors">Enterprise</a>
              <a href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md" className="block text-black/40 hover:text-black transition-colors">Docs</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.1)}>
            <p className="text-[11px] uppercase tracking-[0.25em] text-black/20 mb-4">Community</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://github.com/0xAEQI/aeqi" className="block text-black/40 hover:text-black transition-colors">GitHub</a>
              <a href="https://x.com/0xAEQI" className="block text-black/40 hover:text-black transition-colors">X</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.15)}>
            <p className="text-[11px] uppercase tracking-[0.25em] text-black/20 mb-4">Legal</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://aeqi.ai/terms" className="block text-black/40 hover:text-black transition-colors">Terms</a>
              <a href="https://aeqi.ai/privacy" className="block text-black/40 hover:text-black transition-colors">Privacy</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.2)}>
            <span className="text-[36px] font-bold tracking-tighter text-black leading-none">æ</span>
            <p className="mt-2 text-[12px] text-black/20">
              &copy; {new Date().getFullYear()} aeqi
            </p>
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
      {/* Hero — white */}
      <Nav />
      <Hero />

      {/* Footer — subtle bg, extends to bottom */}
      <div className="bg-black/[0.02] flex-1 flex flex-col">
        <Footer />
      </div>
    </div>
  );
}
