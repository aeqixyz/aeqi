import { motion } from "framer-motion";

export default function Nav() {
  return (
    <motion.nav
      className="fixed top-0 left-0 right-0 z-50 flex justify-center pt-4 px-4"
      initial={{ opacity: 0, y: -20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.6, delay: 0.15, ease: [0.25, 0.1, 0.25, 1] }}
    >
      <div className="w-full max-w-3xl backdrop-blur-2xl bg-white/60 border border-black/[0.06] rounded-2xl shadow-lg shadow-black/[0.03] px-5 h-12 flex items-center justify-between">
        <a href="/" className="text-[18px] font-bold tracking-tighter text-black/60 hover:text-black/80 transition-colors">
          æqi
        </a>
        <div className="flex items-center gap-1">
          <a href="/pricing" className="text-[13px] text-black/55 hover:text-black/80 hover:bg-black/[0.04] rounded-lg px-3 py-1.5 transition-all hidden sm:block">
            Pricing
          </a>
          <div className="w-px h-5 bg-black/[0.08] mx-1.5 hidden sm:block" />
          <a href="https://app.aeqi.ai/login" className="text-[13px] text-black/55 hover:text-black/80 hover:bg-black/[0.04] rounded-lg px-3 py-1.5 transition-all">
            Log in
          </a>
          <a
            href="https://app.aeqi.ai/signup"
            className="bg-black text-white rounded-xl px-4 py-1.5 text-[13px] font-medium hover:bg-black/85 transition-all hover:shadow-md hover:shadow-black/10 active:scale-[0.97]"
          >
            Sign up
          </a>
        </div>
      </div>
    </motion.nav>
  );
}
