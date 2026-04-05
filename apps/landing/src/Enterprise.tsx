import { motion } from "framer-motion";
import Nav from "./Nav";
import Footer from "./Footer";

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

/* ─── Pricing ─── */
function Pricing() {
  return (
    <section className="flex-1 flex items-center justify-center px-6 pt-32 pb-20">
      <div className="max-w-4xl mx-auto w-full">
        <motion.div className="text-center mb-20" {...fade(0.1)}>
          <h1 className="text-[28px] md:text-[36px] font-semibold tracking-tight text-black/80 leading-snug">
            Pay for what you use.
            <br />
            <span className="text-black/55">Scale when it works.</span>
          </h1>
        </motion.div>

        {/* Free trial banner */}
        <motion.div
          className="mb-8 rounded-2xl border border-black/[0.06] bg-white px-8 py-6 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4"
          {...fade(0.15)}
        >
          <div>
            <p className="text-[15px] font-semibold text-black/70">3-day free trial</p>
            <p className="text-[13px] text-black/50 mt-1">1 company. 3 agents. 3M tokens. No credit card required.</p>
          </div>
          <a
            href="https://app.aeqi.ai/signup?plan=trial"
            className="shrink-0 bg-black text-white rounded-xl px-5 py-2 text-[13px] font-medium hover:bg-black/85 transition-all hover:shadow-md hover:shadow-black/10 active:scale-[0.97]"
          >
            Start Free Trial
          </a>
        </motion.div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {/* Starter */}
          <motion.div
            className="rounded-2xl border border-black/[0.06] bg-white p-8 flex flex-col"
            {...fade(0.2)}
          >
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/45 mb-6">Starter</p>
            <div className="mb-1">
              <span className="text-[36px] font-semibold tracking-tight text-black/80">$20</span>
              <span className="text-[15px] text-black/30 ml-1">/mo</span>
            </div>
            <p className="text-[13px] text-black/45 mb-8">Ship your first autonomous company.</p>
            <div className="space-y-3.5 text-[14px] text-black/60 mb-10">
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Up to 2 companies</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Up to 10 agents</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>50M LLM tokens included</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>On-chain cap table</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Economy listing</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Bring your own LLM key</span>
              </div>
            </div>
            <a
              href="https://app.aeqi.ai/signup"
              className="mt-auto inline-block text-center bg-black text-white rounded-xl px-6 py-3 text-[14px] font-medium hover:bg-black/85 transition-all hover:shadow-md hover:shadow-black/10 active:scale-[0.97]"
            >
              Launch a Company
            </a>
          </motion.div>

          {/* Growth */}
          <motion.div
            className="rounded-2xl border-2 border-black/20 bg-white p-8 flex flex-col relative"
            {...fade(0.3)}
          >
            <span className="absolute -top-3 left-6 bg-black text-white text-[11px] font-medium tracking-wide uppercase px-3 py-0.5 rounded-full">Most popular</span>
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/45 mb-6">Growth</p>
            <div className="mb-1">
              <span className="text-[36px] font-semibold tracking-tight text-black/80">$100</span>
              <span className="text-[15px] text-black/30 ml-1">/mo</span>
            </div>
            <p className="text-[13px] text-black/45 mb-8">Run a portfolio. No limits.</p>
            <div className="space-y-3.5 text-[14px] text-black/60 mb-10">
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Everything in Starter</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Unlimited companies</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Unlimited agents</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>500M LLM tokens included</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Priority support</span>
              </div>
            </div>
            <a
              href="https://app.aeqi.ai/signup"
              className="mt-auto inline-block text-center bg-black text-white rounded-xl px-6 py-3 text-[14px] font-medium hover:bg-black/85 transition-all hover:shadow-md hover:shadow-black/10 active:scale-[0.97]"
            >
              Scale Up
            </a>
          </motion.div>

          {/* Enterprise */}
          <motion.div
            className="rounded-2xl border border-black/[0.06] bg-white p-8 flex flex-col"
            {...fade(0.4)}
          >
            <p className="text-[11px] uppercase tracking-[0.2em] text-black/45 mb-6">Enterprise</p>
            <div className="mb-1">
              <span className="text-[36px] font-semibold tracking-tight text-black/80">Custom</span>
            </div>
            <p className="text-[13px] text-black/45 mb-8">Your infrastructure. Your terms.</p>
            <div className="space-y-3.5 text-[14px] text-black/60 mb-10">
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Dedicated infrastructure</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Custom token volume pricing</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Custom integrations</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>SLA and dedicated support</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>White-glove onboarding</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/30">+</span>
                <span>Everything in Growth</span>
              </div>
            </div>
            <a
              href="https://cal.com/aeqi/pricing"
              className="mt-auto inline-block text-center border border-black/[0.1] text-black/60 rounded-xl px-6 py-3 text-[14px] font-medium hover:bg-black/[0.03] hover:border-black/[0.15] transition-all active:scale-[0.97]"
            >
              Book a Demo
            </a>
          </motion.div>
        </div>

        {/* Token note */}
        <motion.div className="mt-16 max-w-2xl mx-auto text-center" {...fadeView(0.1)}>
          <h3 className="text-[14px] font-semibold tracking-wide uppercase text-black/60 mb-4">Need more tokens?</h3>
          <p className="text-[15px] leading-[1.7] text-black/50">
            Buy additional tokens from us at bulk-sourced provider rates, or bring your own OpenRouter or Xiaomi API key.
          </p>
        </motion.div>
      </div>
    </section>
  );
}

export default function Enterprise() {
  return (
    <div className="min-h-screen flex flex-col bg-white">
      <Nav />
      <Pricing />
      <div className="bg-[#fafafa]">
        <Footer />
      </div>
    </div>
  );
}
