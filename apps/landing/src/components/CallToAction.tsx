import { motion } from "framer-motion";

const fadeUp = (delay = 0) => ({
  initial: { opacity: 0, y: 20 },
  whileInView: { opacity: 1, y: 0 },
  viewport: { once: true, margin: "-80px" } as const,
  transition: { duration: 0.6, ease: "easeOut" as const, delay },
});

export function CallToAction() {
  return (
    <section className="relative z-10 max-w-4xl mx-auto px-8 pt-16 pb-12">
      <div className="text-center mb-28">
        <motion.div
          {...fadeUp()}
          className="flex items-center justify-center gap-6 mb-10"
        >
          <a href="https://app.aeqi.ai" className="relative group">
            <div
              className="absolute -inset-1 rounded-full opacity-0 group-hover:opacity-100 blur-md transition-opacity duration-500"
              style={{ background: "linear-gradient(135deg, #c0392b, #e74c3c)" }}
            />
            <div
              className="relative bg-white text-[#08080C] rounded-full px-8 py-3.5 text-[14px] font-medium hover:bg-white/95 transition-colors"
              style={{ fontFamily: "'Space Grotesk', sans-serif" }}
            >
              Use
            </div>
          </a>
          <a
            href="https://github.com/0xAEQI/aeqi"
            className="text-white/30 hover:text-white/60 transition-colors text-[14px]"
          >
            Source
          </a>
        </motion.div>

        <motion.p
          {...fadeUp(0.1)}
          className="text-[13px] text-white/15 tracking-wide"
        >
          Open source · Self-hosted · Built in Rust
        </motion.p>
      </div>

      <div className="h-px bg-white/[0.04] mb-6" />
      <footer
        className="flex items-center justify-between text-[11px] text-white/12 pb-6"
        style={{ fontFamily: "'Space Grotesk', sans-serif" }}
      >
        <span className="tracking-[0.08em]">aeqi.ai</span>
        <div className="flex gap-5">
          <a href="https://github.com/0xAEQI/aeqi" className="hover:text-white/25 transition-colors">GitHub</a>
          <a href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md" className="hover:text-white/25 transition-colors">Docs</a>
        </div>
      </footer>
    </section>
  );
}
