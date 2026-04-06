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
            <p className="text-[12px] uppercase tracking-[0.15em] text-black/50 font-medium mb-4">Product</p>
            <div className="space-y-2.5 text-[14px]">
              <a href="https://app.aeqi.ai/signup" className="block text-black/60 hover:text-black/80 transition-colors">Launch a Company</a>
              <a href="/pricing" className="block text-black/60 hover:text-black/80 transition-colors">Pricing</a>
              <a href="/docs" className="block text-black/60 hover:text-black/80 transition-colors">Docs</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.1)}>
            <p className="text-[12px] uppercase tracking-[0.15em] text-black/50 font-medium mb-4">Community</p>
            <div className="space-y-2.5 text-[14px]">
              <a href="https://github.com/0xAEQI/aeqi" className="block text-black/60 hover:text-black/80 transition-colors">GitHub</a>
              <a href="https://x.com/aeqiai" className="block text-black/60 hover:text-black/80 transition-colors">X</a>
            </div>
          </motion.div>

          <motion.div {...fadeView(0.15)}>
            <p className="text-[12px] uppercase tracking-[0.15em] text-black/50 font-medium mb-4">Legal</p>
            <div className="space-y-2.5 text-[14px]">
              <a href="/terms" className="block text-black/60 hover:text-black/80 transition-colors">Terms</a>
              <a href="/privacy" className="block text-black/60 hover:text-black/80 transition-colors">Privacy</a>
              <a href="/brand" className="block text-black/60 hover:text-black/80 transition-colors">Brand</a>
            </div>
          </motion.div>
        </div>

        <motion.div {...fadeView(0.2)} className="mt-14 pt-6 border-t border-black/[0.04] flex items-center justify-between">
          <a href="/" className="text-[18px] font-bold tracking-[-0.08em] text-black/50 hover:text-black/70 transition-colors">æq<span className="inline-block translate-y-[0.04em]">i</span></a>
          <p className="text-[12px] text-black/45">
            &copy; {new Date().getFullYear()} aeqi.ai
          </p>
        </motion.div>
      </div>
    </footer>
  );
}
