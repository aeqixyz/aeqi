import { useState, useEffect } from "react";
import { Hero } from "./components/Hero";
import { VerticalLines } from "./components/VerticalLines";
import { motion } from "framer-motion";

function GitHubStars() {
  const [stars, setStars] = useState<number | null>(null);

  useEffect(() => {
    fetch("https://api.github.com/repos/0xAEQI/aeqi")
      .then((r) => r.json())
      .then((d) => {
        if (typeof d.stargazers_count === "number") setStars(d.stargazers_count);
      })
      .catch(() => {});
  }, []);

  return (
    <a
      href="https://github.com/0xAEQI/aeqi"
      className="flex items-center gap-2 text-white/35 hover:text-white/60 transition-colors"
    >
      <svg viewBox="0 0 16 16" fill="currentColor" className="w-[16px] h-[16px]">
        <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z" />
      </svg>
      <span className="text-[13px]">star</span>
      {stars !== null && (
        <span className="text-[10px] bg-white/[0.06] px-1.5 py-0.5 rounded">
          {stars}
        </span>
      )}
    </a>
  );
}

function Nav() {
  return (
    <motion.nav
      className="fixed top-5 left-1/2 -translate-x-1/2 z-50"
      initial={{ opacity: 0, y: -10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.6, delay: 0.4, ease: "easeOut" }}
    >
      <div
        className="backdrop-blur-2xl bg-white/[0.03] border border-white/[0.07] rounded-full px-7 py-3.5 flex items-center gap-7"

      >
        <a
          href="/"
          className="text-[18px] font-bold tracking-[0.06em] hover:opacity-80 transition-opacity"
        >
          <span className="text-[#c0392b]">aeqi</span><span className="text-white/30">.</span><span className="text-white">ai</span>
        </a>
        <div className="flex items-center gap-4">
          <a
            href="mailto:enterprise@aeqi.ai"
            className="text-[14px] text-white/35 hover:text-white/60 transition-colors"
          >
            enterprise
          </a>
          <a
            href="https://app.aeqi.ai"
            className="bg-white text-[#06060E] rounded-full px-6 py-2.5 text-[14px] font-semibold hover:bg-white/90 transition-colors"
    
          >
            Get Started
          </a>
        </div>
      </div>
    </motion.nav>
  );
}

function Backdrop() {
  return (
    <>
      <div
        className="fixed inset-0 z-0 bg-cover bg-no-repeat"
        style={{
          backgroundImage: "url('/bg.jpg')",
          backgroundPosition: "center 35%",
          filter: "blur(6px) saturate(1.4) brightness(0.3) contrast(1.3)",
          transform: "scale(1.03)",
        }}
      />
      <div
        className="fixed inset-0 z-0"
        style={{ background: "rgba(6, 6, 18, 0.5)", mixBlendMode: "multiply" }}
      />
      <div
        className="fixed inset-0 z-0"
        style={{
          background: "radial-gradient(ellipse 70% 60% at 50% 45%, transparent 0%, rgba(6,6,18,0.85) 100%)",
        }}
      />
      <div
        className="fixed inset-0 z-0"
        style={{
          background: "linear-gradient(to bottom, rgba(6,6,18,0.7) 0%, transparent 25%)",
        }}
      />
    </>
  );
}

function Footer() {
  return (
    <motion.footer
      className="fixed bottom-5 left-1/2 -translate-x-1/2 z-50"
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.6, delay: 1.2, ease: "easeOut" }}
    >
      <div
        className="backdrop-blur-2xl bg-white/[0.03] border border-white/[0.07] rounded-full px-7 py-3 flex items-center gap-6 text-[13px] text-white/25"

      >
        <span className="tracking-[0.08em]"><span className="text-[#c0392b]">aeqi</span><span className="text-white/30">.</span><span className="text-white">ai</span></span>
        <div className="w-px h-3.5 bg-white/[0.08]" />
        <GitHubStars />
        <a href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md" className="hover:text-white/50 transition-colors">docs</a>
        <div className="w-px h-3.5 bg-white/[0.08]" />
        <span className="text-white/15">open source · rust</span>
      </div>
    </motion.footer>
  );
}

export default function App() {
  return (
    <div className="relative min-h-screen">
      <Backdrop />
      <VerticalLines />
      <Nav />
      <Hero />
      <Footer />
    </div>
  );
}
