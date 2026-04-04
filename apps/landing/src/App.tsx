import { useState } from "react";
import { motion } from "framer-motion";

/* ─── Fade-in helper ─── */
const fade = (delay = 0) => ({
  initial: { opacity: 0, y: 8 } as const,
  animate: { opacity: 1, y: 0 } as const,
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
        <a href="/" className="text-[16px] font-medium tracking-tight text-black/40">
          æqi.ai
        </a>
        <div className="flex items-center gap-5">
          <a
            href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md"
            className="text-[14px] text-black/40 hover:text-black/70 transition-colors"
          >
            docs
          </a>
          <a
            href="https://github.com/0xAEQI/aeqi"
            className="text-[14px] text-black/40 hover:text-black/70 transition-colors"
          >
            github
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
    <section className="pt-36 pb-24 px-6">
      <div className="max-w-3xl mx-auto text-center">
        <motion.div {...fade(0.1)}>
          <span className="text-[120px] md:text-[180px] font-bold tracking-tighter leading-none text-black select-none">
            æ
          </span>
        </motion.div>

        <motion.p
          className="mt-5 text-lg md:text-xl text-black/40 tracking-wide"
          {...fade(0.35)}
        >
          agent orchestration kernel
        </motion.p>

        <motion.p
          className="mt-8 text-[15px] text-black/30 tracking-[0.04em]"
          {...fade(0.5)}
        >
          agent&ensp;&middot;&ensp;event&ensp;&middot;&ensp;quest&ensp;&middot;&ensp;insight
        </motion.p>

        <motion.div className="mt-10" {...fade(0.6)}>
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
    <footer className="border-t border-black/5 py-8 px-6">
      <div className="max-w-5xl mx-auto flex items-center justify-between text-[13px] text-black/25">
        <span className="font-medium tracking-tight">aeqi.ai</span>
        <div className="flex items-center gap-5">
          <a
            href="https://github.com/0xAEQI/aeqi"
            className="hover:text-black/50 transition-colors"
          >
            github
          </a>
          <a
            href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md"
            className="hover:text-black/50 transition-colors"
          >
            docs
          </a>
          <span className="text-black/15">open source &middot; rust</span>
        </div>
      </div>
    </footer>
  );
}

/* ─── App ─── */
export default function App() {
  return (
    <div className="min-h-screen bg-white">
      <Nav />
      <Hero />
      <Footer />
    </div>
  );
}
