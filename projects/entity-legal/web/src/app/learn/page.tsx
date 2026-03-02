import type { Metadata } from "next";
import Link from "next/link";
import { Footer } from "@/components/Footer";

export const metadata: Metadata = {
  title: "Knowledge Base — entity.legal",
  description:
    "Comprehensive legal reference for Marshall Islands DAO LLC formation, Series LLC structures, tax treatment, compliance requirements, and jurisdiction comparisons.",
  alternates: { canonical: "https://entity.legal/learn" },
};

const articles = [
  {
    href: "/marshall-islands-dao-llc",
    title: "Marshall Islands DAO LLC",
    subtitle: "The definitive guide to the RMI DAO legislative framework — from the 2022 Act through the 2024 Regulations.",
    tag: "Pillar",
  },
  {
    href: "/series-dao-llc",
    title: "Series DAO LLC",
    subtitle: "How the Series LLC structure enables segregated assets, ring-fenced liability, and multi-protocol ecosystems under a single parent entity.",
    tag: "Structure",
  },
  {
    href: "/dao-llc-tax",
    title: "DAO LLC Tax Structure",
    subtitle: "For-profit vs. non-profit election, the 3% Gross Revenue Tax, territorial exemptions, and 0% non-profit treatment.",
    tag: "Tax",
  },
  {
    href: "/dao-llc-compliance",
    title: "DAO LLC Compliance & KYC",
    subtitle: "Beneficial Owner Information Reports, KYC thresholds, the Foreign Investment Business License, and on-chain monitoring requirements.",
    tag: "Compliance",
  },
  {
    href: "/dao-llc-vs-wyoming",
    title: "RMI vs. Wyoming vs. Cayman vs. Delaware",
    subtitle: "Jurisdiction comparison across taxation, membership requirements, governance flexibility, and regulatory risk.",
    tag: "Comparison",
  },
  {
    href: "/ai-agent-legal-entity",
    title: "Legal Entities for AI Agents",
    subtitle: "Why autonomous AI systems need legal personhood, and how the Marshall Islands DAO LLC framework enables machine incorporation.",
    tag: "AI",
  },
  {
    href: "/dao-llc-banking",
    title: "DAO LLC Banking",
    subtitle: "How to open bank accounts, obtain debit cards, and manage crypto wallets for your Marshall Islands DAO LLC.",
    tag: "Banking",
  },
  {
    href: "/offshore-dao-myths",
    title: "Offshore DAO Myths",
    subtitle: "EU tax blacklists, CFC rules, banking access, and management location — separating legitimate concerns from fear-based misinformation.",
    tag: "Myths",
  },
];

export default function LearnPage() {
  return (
    <div className="min-h-screen bg-bg-primary">
      <header className="border-b border-border px-6 py-4">
        <div className="mx-auto flex max-w-[720px] items-center justify-between">
          <Link href="/" className="font-serif text-[14px] tracking-[0.12em] text-text-secondary transition-colors hover:text-text-primary">
            entity<span className="text-text-tertiary">.</span>legal
          </Link>
          <span className="text-[12px] text-text-tertiary">Knowledge Base</span>
        </div>
      </header>

      <nav className="px-6 pt-6" aria-label="Breadcrumb">
        <div className="mx-auto max-w-[720px] flex items-center gap-2 text-[12px] text-text-tertiary">
          <Link href="/" className="transition-colors hover:text-text-secondary">Home</Link>
          <span>/</span>
          <span className="text-text-muted">Knowledge Base</span>
        </div>
      </nav>

      <main className="px-6 py-16 md:py-20">
        <div className="mx-auto max-w-[720px]">
          <h1 className="font-serif text-[clamp(28px,4vw,42px)] leading-[1.2] text-text-primary">
            Knowledge Base
          </h1>
          <p className="mt-4 max-w-[520px] text-[16px] leading-[1.6] text-text-secondary">
            Authoritative legal reference for Marshall Islands DAO LLC formation, governance, taxation, and compliance.
          </p>

          <div className="mt-14 space-y-0">
            {articles.map((a) => (
              <Link key={a.href} href={a.href} className="group block border-b border-border py-6 transition-colors first:border-t">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <div className="flex items-center gap-3">
                      <h2 className="font-serif text-[20px] text-text-primary transition-colors group-hover:text-white">
                        {a.title}
                      </h2>
                      <span className="rounded-full border border-border px-2.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.15em] text-text-tertiary">
                        {a.tag}
                      </span>
                    </div>
                    <p className="mt-1.5 text-[14px] leading-[1.6] text-text-tertiary transition-colors group-hover:text-text-secondary">
                      {a.subtitle}
                    </p>
                  </div>
                  <span className="mt-1 text-text-tertiary transition-transform group-hover:translate-x-1">&rarr;</span>
                </div>
              </Link>
            ))}
          </div>
        </div>
      </main>

      <Footer />
    </div>
  );
}
