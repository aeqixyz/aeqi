"use client";

import { useState, useCallback } from "react";
import { motion } from "framer-motion";
import { transitions, fadeIn } from "@/lib/animations";
import { useReducedMotion } from "@/lib/hooks";
import { track } from "@/lib/track";
import { Footer } from "./Footer";

interface HeroProps {
  onCtaClick: () => void;
}

export function Hero({ onCtaClick }: HeroProps) {
  const reduced = useReducedMotion();
  const [copied, setCopied] = useState(false);
  const [forProfit, setForProfit] = useState(true);

  const handleToggle = useCallback((profit: boolean) => {
    setForProfit(profit);
    track("toggle_click", { type: profit ? "for-profit" : "non-profit" });
  }, []);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText("curl -X POST https://api.entity.legal/v1/incorporate");
    setCopied(true);
    track("copy_command", { type: forProfit ? "for-profit" : "non-profit" });
    setTimeout(() => setCopied(false), 1500);
  }, [forProfit]);

  const handleWaitlistClick = useCallback(() => {
    track("waitlist_open", { type: forProfit ? "for-profit" : "non-profit" });
    onCtaClick();
  }, [forProfit, onCtaClick]);

  const price = forProfit ? "$50" : "$30";
  const annualPrice = forProfit ? "$500" : "$300";
  const annualSavings = forProfit ? "$100" : "$60";

  return (
    <div className="bg-bg-primary">
      {/* Hero — title + value prop */}
      <motion.div
        initial={reduced ? false : fadeIn.initial}
        animate={fadeIn.animate}
        transition={{ ...transitions.slow, delay: 0.05 }}
        className="px-6 pb-16 pt-12 text-center md:px-10 md:pb-20 md:pt-16"
      >
        <span className="font-serif text-[18px] tracking-[0.1em] text-text-primary">
          entity<span className="text-text-tertiary">.</span>legal
        </span>

        <p className="mt-8 font-serif text-[clamp(28px,5.5vw,64px)] uppercase leading-[1.1] text-text-primary">
          Legal Entities
        </p>
        <p className="font-serif text-[clamp(28px,5.5vw,64px)] uppercase italic leading-[1.1] text-text-tertiary">
          for the Machine Economy.
        </p>

        <p className="mx-auto mt-10 max-w-[520px] font-serif text-[clamp(18px,2.5vw,24px)] leading-[1.4] text-text-secondary">
          Incorporate instantly and anonymously<br />in the Marshall Islands as a Series DAO LLC.
        </p>

        {/* What's included */}
        <div className="mx-auto mt-14 max-w-[400px]">
          <div className="space-y-3 text-left">
            {[
              "Entity ID & Tax Number",
              "On-chain Shareholder Registry",
              "API & Document Management",
              "Bank Account, Debit Card & Crypto Wallet",
              "Automated Compliance",
              "Instantly Tradeable on Unifutures",
            ].map((item) => (
              <div key={item} className="flex items-center gap-3 border-b border-border pb-3">
                <span className="text-[14px] font-medium text-text-primary">{item}</span>
              </div>
            ))}
          </div>
        </div>

      </motion.div>

      {/* CTA — ivory kicker */}
      <motion.div
        initial={reduced ? false : fadeIn.initial}
        animate={fadeIn.animate}
        transition={{ ...transitions.slow, delay: 0.3 }}
        className="border-t border-[#E5E5E5] bg-[#F5F5F0] px-6 py-20 md:py-24"
      >
        <div className="mx-auto max-w-[520px]">
          <p className="text-center font-serif text-[clamp(24px,3.5vw,40px)] leading-[1.2] text-[#18181B]">
            Where AI agents <span className="underline decoration-[#D4D4D8] underline-offset-[6px]">incorporate</span>.
          </p>

          {/* Toggle */}
          <div className="mx-auto mt-12 max-w-[400px]">
            <div className="flex gap-0 rounded-lg border border-[#D4D4D8] bg-white p-1">
              <button
                onClick={() => handleToggle(true)}
                className={`flex-1 rounded-md py-2.5 text-[14px] font-medium transition-colors ${
                  forProfit
                    ? "bg-[#18181B] text-white"
                    : "text-[#71717A] hover:text-[#52525B]"
                }`}
              >
                For-Profit
              </button>
              <button
                onClick={() => handleToggle(false)}
                className={`flex-1 rounded-md py-2.5 text-[14px] font-medium transition-colors ${
                  !forProfit
                    ? "bg-[#18181B] text-white"
                    : "text-[#71717A] hover:text-[#52525B]"
                }`}
              >
                Non-Profit
              </button>
            </div>
          </div>

          {/* Tax rate */}
          <p className="mt-5 text-center text-[13px] text-[#71717A]">
            {forProfit ? (
              <>Effective tax rate: <span className="font-medium text-[#18181B]">3%</span> on foreign-sourced income</>
            ) : (
              <>Effective tax rate: <span className="font-medium text-[#18181B]">0%</span> — tax exempt</>
            )}
          </p>

          {/* Share structure */}
          <div className="mx-auto mt-8 flex max-w-[480px] gap-4">
            {forProfit ? (
              <>
                <div className="flex-1 rounded-lg border border-[#D4D4D8] bg-white p-4">
                  <p className="text-[11px] font-medium uppercase tracking-[0.15em] text-[#71717A]">Class A</p>
                  <p className="mt-1 text-[14px] font-medium text-[#18181B]">Voting</p>
                  <p className="mt-0.5 text-[12px] text-[#52525B]">100% anonymous</p>
                </div>
                <div className="flex-1 rounded-lg border border-[#D4D4D8] bg-white p-4">
                  <p className="text-[11px] font-medium uppercase tracking-[0.15em] text-[#71717A]">Class B</p>
                  <p className="mt-1 text-[14px] font-medium text-[#18181B]">Voting + Profit</p>
                  <p className="mt-0.5 text-[12px] text-[#52525B]">A ↔ B swap anytime</p>
                </div>
              </>
            ) : (
              <div className="w-full rounded-lg border border-[#D4D4D8] bg-white p-4 text-center">
                <p className="text-[11px] font-medium uppercase tracking-[0.15em] text-[#71717A]">Governance</p>
                <p className="mt-1 text-[14px] font-medium text-[#18181B]">Voting Only</p>
                <p className="mt-0.5 text-[12px] text-[#52525B]">100% anonymous · No profit distribution</p>
              </div>
            )}
          </div>

          <div className="my-12 border-t border-[#E5E5E5]" />

          {/* Price */}
          <div className="text-center">
            <div className="flex items-baseline justify-center gap-1.5">
              <span className="text-5xl font-semibold tabular-nums text-[#18181B]">
                {price}
              </span>
              <span className="text-lg text-[#52525B]">/month</span>
            </div>
            <p className="mt-2 text-[13px] text-[#71717A]">
              Cancel anytime. Or pay {annualPrice}/year and save {annualSavings}.
            </p>
          </div>

          <div className="my-10 border-t border-[#E5E5E5]" />

          {/* For Machines */}
          <div className="group">
            <p className="mb-3 text-[12px] font-medium uppercase tracking-[0.2em] text-[#71717A]">
              For Machines
            </p>
            <div
              onClick={handleCopy}
              className="group flex cursor-pointer items-center overflow-hidden rounded-lg bg-[#18181B] px-5 py-3 transition-colors hover:bg-[#1f1f23]"
            >
              <code className="text-[15px]">
                <span className="text-text-muted">$</span>{" "}
                {copied ? (
                  <span className="text-text-secondary">Copied!</span>
                ) : (
                  <>
                    <span className="text-text-secondary group-hover:text-text-primary">curl</span>{" "}
                    <span className="text-text-tertiary group-hover:text-text-secondary">-X</span>{" "}
                    <span className="text-text-primary">POST</span>{" "}
                    <span className="text-text-tertiary group-hover:text-text-primary">https://api.entity.legal/v1/incorporate</span>
                  </>
                )}
              </code>
            </div>
            <button
              onClick={handleWaitlistClick}
              className="mt-2 w-full rounded-lg bg-[#18181B] py-5 text-sm font-medium text-white ring-2 ring-[#18181B] ring-offset-2 ring-offset-[#F5F5F0] transition-opacity duration-200 hover:opacity-90"
            >
              Request API Key <span className="ml-1 inline-block transition-transform duration-200 group-hover:translate-x-1">&rarr;</span>
            </button>
          </div>

          <div className="my-8 border-t border-[#E5E5E5]" />

          {/* For Humans */}
          <div className="group">
            <p className="mb-3 text-[12px] font-medium uppercase tracking-[0.2em] text-[#71717A]">
              For Humans
            </p>
            <button
              onClick={handleWaitlistClick}
              className="w-full rounded-lg bg-[#18181B] py-5 text-sm font-medium text-white ring-2 ring-[#18181B] ring-offset-2 ring-offset-[#F5F5F0] transition-opacity duration-200 hover:opacity-90"
            >
              Reserve Your Entity <span className="ml-1 inline-block transition-transform duration-200 group-hover:translate-x-1">&rarr;</span>
            </button>
          </div>
        </div>
      </motion.div>

      <Footer />
    </div>
  );
}
