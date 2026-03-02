"use client";

import { useState } from "react";
import Link from "next/link";
import { WaitlistModal } from "./WaitlistModal";
import { track } from "@/lib/track";
import { Footer } from "./Footer";

const allArticles = [
  { href: "/marshall-islands-dao-llc", title: "Marshall Islands DAO LLC", tag: "Pillar" },
  { href: "/series-dao-llc", title: "Series DAO LLC", tag: "Structure" },
  { href: "/dao-llc-tax", title: "DAO LLC Tax Structure", tag: "Tax" },
  { href: "/dao-llc-compliance", title: "DAO LLC Compliance & KYC", tag: "Compliance" },
  { href: "/dao-llc-vs-wyoming", title: "RMI vs. Wyoming vs. Cayman vs. Delaware", tag: "Comparison" },
  { href: "/ai-agent-legal-entity", title: "Legal Entities for AI Agents", tag: "AI" },
  { href: "/dao-llc-banking", title: "DAO LLC Banking", tag: "Banking" },
  { href: "/offshore-dao-myths", title: "Offshore DAO Myths", tag: "Myths" },
];

interface ArticleProps {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  publishedDate?: string;
  updatedDate?: string;
  slug?: string;
}

export function Article({ title, subtitle, children, publishedDate, updatedDate, slug }: ArticleProps) {
  const [waitlistOpen, setWaitlistOpen] = useState(false);
  const relatedArticles = allArticles.filter((a) => a.href !== `/${slug}`);

  return (
    <div className="bg-bg-primary min-h-screen">
      <header className="border-b border-border px-6 py-4">
        <div className="mx-auto flex max-w-[720px] items-center justify-between">
          <Link href="/" className="font-serif text-[14px] tracking-[0.12em] text-text-secondary transition-colors hover:text-text-primary">
            entity<span className="text-text-tertiary">.</span>legal
          </Link>
          <Link href="/learn" className="text-[12px] text-text-tertiary transition-colors hover:text-text-secondary">
            Knowledge Base
          </Link>
        </div>
      </header>

      {/* Breadcrumb */}
      <nav className="px-6 pt-6" aria-label="Breadcrumb">
        <div className="mx-auto max-w-[720px] flex items-center gap-2 text-[12px] text-text-tertiary">
          <Link href="/" className="transition-colors hover:text-text-secondary">Home</Link>
          <span>/</span>
          <Link href="/learn" className="transition-colors hover:text-text-secondary">Learn</Link>
          <span>/</span>
          <span className="text-text-muted truncate">{title}</span>
        </div>
      </nav>

      <article className="px-6 py-12 md:py-16">
        <div className="mx-auto max-w-[720px]">
          <h1 className="font-serif text-[clamp(28px,4vw,42px)] leading-[1.2] text-text-primary">
            {title}
          </h1>
          {subtitle && (
            <p className="mt-4 text-[18px] leading-[1.5] text-text-secondary">
              {subtitle}
            </p>
          )}
          {(publishedDate || updatedDate) && (
            <div className="mt-4 flex gap-4 text-[12px] text-text-tertiary">
              {publishedDate && <span>Published {publishedDate}</span>}
              {updatedDate && <span>Updated {updatedDate}</span>}
            </div>
          )}

          <div className="article-content mt-12">
            {children}
          </div>

          {/* CTA */}
          <div className="mt-16 rounded-lg border border-border bg-bg-card p-8 text-center">
            <p className="font-serif text-[24px] text-text-primary">
              Ready to incorporate?
            </p>
            <p className="mt-2 text-[14px] text-text-secondary">
              Form your Marshall Islands Series DAO LLC instantly and anonymously.
            </p>
            <button
              onClick={() => { track("article_cta_click", { article: title }); setWaitlistOpen(true); }}
              className="mt-6 rounded-lg bg-[#FAFAFA] px-8 py-3 text-[14px] font-medium text-[#09090B] transition-opacity hover:opacity-90"
            >
              Get Started &rarr;
            </button>
          </div>

          {/* Related articles */}
          <div className="mt-16">
            <h2 className="text-[12px] font-medium uppercase tracking-[0.2em] text-text-tertiary">
              Continue Reading
            </h2>
            <div className="mt-4 space-y-0">
              {relatedArticles.map((a) => (
                <Link key={a.href} href={a.href} className="group flex items-center justify-between border-b border-border py-4 transition-colors first:border-t">
                  <div className="flex items-center gap-3">
                    <span className="text-[14px] text-text-secondary transition-colors group-hover:text-text-primary">
                      {a.title}
                    </span>
                    <span className="rounded-full border border-border px-2 py-0.5 text-[10px] uppercase tracking-[0.1em] text-text-muted">
                      {a.tag}
                    </span>
                  </div>
                  <span className="text-[12px] text-text-tertiary transition-transform group-hover:translate-x-1">&rarr;</span>
                </Link>
              ))}
            </div>
          </div>
        </div>
      </article>

      <Footer />

      <WaitlistModal isOpen={waitlistOpen} onClose={() => setWaitlistOpen(false)} />
    </div>
  );
}
