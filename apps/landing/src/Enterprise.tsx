import { motion } from "framer-motion";
import Nav from "./Nav";
import Footer from "./Footer";
import { PLANS, TRIAL } from "../../shared/pricing";

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

const starter = PLANS[0];
const growth = PLANS[1];

/* --- Pricing --- */
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
            <p className="text-[15px] font-semibold text-black/70">{TRIAL.days}-day free trial</p>
            <p className="text-[14px] text-black/50 mt-1">{TRIAL.companies} company. {TRIAL.agents} agents. {TRIAL.tokens} tokens. No credit card required.</p>
          </div>
          <a
            href="https://app.aeqi.ai/signup?plan=trial"
            className="shrink-0 bg-black text-white rounded-xl px-5 py-2 text-[14px] font-medium hover:bg-black/85 transition-all hover:shadow-md hover:shadow-black/10 active:scale-[0.97]"
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
            <p className="text-[12px] uppercase tracking-[0.15em] text-black/50 font-medium mb-6">{starter.name}</p>
            <div className="mb-1">
              <span className="text-[36px] font-semibold tracking-tight text-black/80">${starter.price}</span>
              <span className="text-[14px] text-black/40 ml-1">/mo</span>
            </div>
            <p className="text-[14px] text-black/50 mb-8 min-h-[40px]">{starter.tagline}</p>
            <div className="space-y-3.5 text-[15px] text-black/60 mb-10">
              {starter.features.map((f) => (
                <div key={f.text} className="flex items-center gap-2.5">
                  <span className="text-black/40">+</span>
                  <span>{f.text}</span>
                </div>
              ))}
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
            <span className="absolute -top-3 left-6 bg-black text-white text-[11px] font-semibold tracking-wide uppercase px-3 py-0.5 rounded-full">Most popular</span>
            <p className="text-[12px] uppercase tracking-[0.15em] text-black/50 font-medium mb-6">{growth.name}</p>
            <div className="mb-1">
              <span className="text-[36px] font-semibold tracking-tight text-black/80">${growth.price}</span>
              <span className="text-[14px] text-black/40 ml-1">/mo</span>
            </div>
            <p className="text-[14px] text-black/50 mb-8 min-h-[40px]">{growth.tagline}</p>
            <div className="space-y-3.5 text-[15px] mb-10">
              {growth.features.map((f) => (
                <div key={f.text} className={`flex items-center gap-2.5 ${f.highlight ? "text-black/80 font-medium" : "text-black/60"}`}>
                  <span className={f.highlight ? "text-black/60" : "text-black/40"}>+</span>
                  <span>{f.text}</span>
                </div>
              ))}
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
            <p className="text-[12px] uppercase tracking-[0.15em] text-black/50 font-medium mb-6">Enterprise</p>
            <div className="mb-1">
              <span className="text-[36px] font-semibold tracking-tight text-black/80">Custom</span>
            </div>
            <p className="text-[14px] text-black/50 mb-8 min-h-[40px]">Your infrastructure. Your terms.</p>
            <div className="space-y-3.5 text-[15px] text-black/60 mb-10">
              <div className="flex items-center gap-2.5">
                <span className="text-black/40">+</span>
                <span>Dedicated infrastructure</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/40">+</span>
                <span>Custom token volume pricing</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/40">+</span>
                <span>Custom integrations</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/40">+</span>
                <span>SLA and dedicated support</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/40">+</span>
                <span>White-glove onboarding</span>
              </div>
              <div className="flex items-center gap-2.5">
                <span className="text-black/40">+</span>
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

        {/* Token options */}
        <motion.div className="mt-20 pt-16 border-t border-black/[0.06]" {...fadeView(0.1)}>
          <p className="text-[11px] font-medium uppercase tracking-[0.15em] text-black/30 text-center mb-8">Tokens</p>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 max-w-2xl mx-auto">
            <div className="rounded-2xl border border-black/[0.06] bg-white p-6">
              <p className="text-[15px] font-semibold text-black/75 mb-2">Buy from us</p>
              <p className="text-[13px] text-black/45 leading-relaxed">
                Additional tokens at bulk-sourced provider rates. No markup. Billed monthly with your plan.
              </p>
            </div>
            <div className="rounded-2xl border border-black/[0.06] bg-white p-6">
              <p className="text-[15px] font-semibold text-black/75 mb-2">Bring your own key</p>
              <p className="text-[13px] text-black/45 leading-relaxed">
                Connect your OpenRouter, Anthropic, or Ollama API key. Use your own models. Pay your provider directly.
              </p>
            </div>
          </div>
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
