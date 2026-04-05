import { motion } from "framer-motion";

const fadeView = (delay = 0) => ({
  initial: { opacity: 0, y: 16 } as const,
  whileInView: { opacity: 1, y: 0 } as const,
  viewport: { once: true, margin: "-40px" } as const,
  transition: { duration: 0.7, ease: [0.25, 0.1, 0.25, 1] as const, delay },
});

export default function Footer() {
  return (
    <footer className="border-t border-black/[0.04]">
      <div className="max-w-4xl mx-auto px-6 py-14 w-full">
        <div className="grid grid-cols-2 md:grid-cols-3 gap-10 md:gap-14">
          <motion.div {...fadeView(0.05)}>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/50 mb-4">Product</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://app.aeqi.ai/signup" className="block text-black/60 hover:text-black/80 transition-colors">Launch a Company</a>
              <a href="/pricing" className="block text-black/60 hover:text-black/80 transition-colors">Pricing</a>
              <a href="https://github.com/0xAEQI/aeqi/blob/main/docs/architecture.md" className="block text-black/60 hover:text-black/80 transition-colors">Docs</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.1)}>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/50 mb-4">Community</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="https://github.com/0xAEQI/aeqi" className="block text-black/60 hover:text-black/80 transition-colors">GitHub</a>
              <a href="https://x.com/0xAEQI" className="block text-black/60 hover:text-black/80 transition-colors">X</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.15)}>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/50 mb-4">Legal</p>
            <div className="space-y-2.5 text-[13px]">
              <a href="/terms" className="block text-black/60 hover:text-black/80 transition-colors">Terms</a>
              <a href="/privacy" className="block text-black/60 hover:text-black/80 transition-colors">Privacy</a>
            </div>
          </motion.div>
        </div>

        <motion.div {...fadeView(0.2)} className="mt-14 pt-6 border-t border-black/[0.04] flex items-center justify-between">
          <a href="/" className="text-[18px] font-bold tracking-tighter text-black/40 hover:text-black/60 transition-colors">æqi</a>
          <p className="text-[12px] text-black/35">
            &copy; {new Date().getFullYear()} aeqi.ai
          </p>
        </motion.div>
      </div>
    </footer>
  );
}
