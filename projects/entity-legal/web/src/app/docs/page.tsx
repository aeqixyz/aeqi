import type { Metadata } from "next";
import Link from "next/link";
import { Footer } from "@/components/Footer";

export const metadata: Metadata = {
  title: "API Documentation — entity.legal",
  description:
    "REST API documentation for entity.legal. Incorporate legal entities programmatically in the Marshall Islands as a Series LLC.",
  alternates: { canonical: "https://entity.legal/docs" },
};

const endpoints = [
  {
    method: "POST",
    path: "/v1/incorporate",
    description: "Form a new legal entity in the Marshall Islands.",
    status: "preview",
    request: `curl -X POST https://api.entity.legal/v1/incorporate \\
  -H "Authorization: Bearer YOUR_API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "type": "for-profit",
    "name": "My Entity LLC",
    "shares": { "class_a": 1000, "class_b": 0 }
  }'`,
    response: `{
  "status": "queued",
  "entity_id": "ent_a1b2c3d4",
  "jurisdiction": "Marshall Islands",
  "structure": "Series DAO LLC",
  "name": "My Entity LLC",
  "tax_id": "MH-2026-XXXXX",
  "shares": {
    "class_a": 1000,
    "class_b": 0
  }
}`,
  },
  {
    method: "GET",
    path: "/v1/entities/:id",
    description: "Retrieve entity details including status, formation date, and share structure.",
    status: "coming soon",
    request: `curl https://api.entity.legal/v1/entities/ent_a1b2c3d4 \\
  -H "Authorization: Bearer YOUR_API_KEY"`,
    response: `{
  "id": "ent_a1b2c3d4",
  "name": "My Entity LLC",
  "status": "active",
  "jurisdiction": "Marshall Islands",
  "type": "for-profit",
  "formed_at": "2026-02-28T00:00:00Z",
  "tax_id": "MH-2026-XXXXX",
  "shares": {
    "class_a": { "total": 1000, "holders": 1 },
    "class_b": { "total": 0, "holders": 0 }
  }
}`,
  },
  {
    method: "GET",
    path: "/v1/entities/:id/shares",
    description: "View the on-chain cap table — all shareholders and their holdings.",
    status: "coming soon",
    request: `curl https://api.entity.legal/v1/entities/ent_a1b2c3d4/shares \\
  -H "Authorization: Bearer YOUR_API_KEY"`,
    response: `{
  "entity_id": "ent_a1b2c3d4",
  "total_shares": 1000,
  "classes": [
    {
      "class": "A",
      "type": "voting",
      "total": 1000,
      "holders": [
        { "address": "0x...", "amount": 1000 }
      ]
    }
  ],
  "registry": "solana",
  "registry_address": "..."
}`,
  },
];

export default function DocsPage() {
  return (
    <div className="min-h-screen bg-bg-primary">
      <header className="border-b border-border px-6 py-4">
        <div className="mx-auto flex max-w-[800px] items-center justify-between">
          <Link href="/" className="font-serif text-[14px] tracking-[0.12em] text-text-secondary transition-colors hover:text-text-primary">
            entity<span className="text-text-tertiary">.</span>legal
          </Link>
          <span className="text-[12px] text-text-tertiary">API Documentation</span>
        </div>
      </header>

      <nav className="px-6 pt-6" aria-label="Breadcrumb">
        <div className="mx-auto max-w-[800px] flex items-center gap-2 text-[12px] text-text-tertiary">
          <Link href="/" className="transition-colors hover:text-text-secondary">Home</Link>
          <span>/</span>
          <span className="text-text-muted">API Documentation</span>
        </div>
      </nav>

      <main className="px-6 py-16 md:py-20">
        <div className="mx-auto max-w-[800px]">
          <h1 className="font-serif text-[clamp(28px,4vw,42px)] leading-[1.2] text-text-primary">
            API Reference
          </h1>
          <p className="mt-4 text-[16px] leading-[1.6] text-text-secondary">
            Incorporate legal entities programmatically. One POST request = one entity.
          </p>

          <div className="mt-8 rounded-lg border border-border bg-bg-card p-4">
            <p className="text-[12px] font-medium uppercase tracking-[0.15em] text-text-tertiary">Base URL</p>
            <code className="mt-1 block text-[15px] text-text-primary">https://api.entity.legal</code>
          </div>

          <div className="mt-4 rounded-lg border border-border bg-bg-card p-4">
            <p className="text-[12px] font-medium uppercase tracking-[0.15em] text-text-tertiary">Authentication</p>
            <code className="mt-1 block text-[14px] text-text-secondary">Authorization: Bearer YOUR_API_KEY</code>
            <p className="mt-2 text-[13px] text-text-muted">
              API keys are not yet available. <Link href="/" className="text-text-secondary underline underline-offset-2">Join the waitlist</Link> to request access.
            </p>
          </div>

          <div className="mt-16 space-y-16">
            {endpoints.map((ep) => (
              <div key={ep.path} id={ep.path.replace(/[/:]/g, "-")}>
                <div className="flex items-center gap-3">
                  <span className={`rounded px-2 py-0.5 text-[12px] font-bold ${
                    ep.method === "POST"
                      ? "bg-green-900/30 text-green-400"
                      : "bg-blue-900/30 text-blue-400"
                  }`}>
                    {ep.method}
                  </span>
                  <code className="text-[16px] text-text-primary">{ep.path}</code>
                  <span className="rounded-full border border-border px-2 py-0.5 text-[10px] uppercase tracking-[0.1em] text-text-muted">
                    {ep.status}
                  </span>
                </div>
                <p className="mt-2 text-[14px] text-text-secondary">{ep.description}</p>

                <div className="mt-4">
                  <p className="mb-2 text-[11px] font-medium uppercase tracking-[0.15em] text-text-tertiary">Request</p>
                  <pre className="overflow-x-auto rounded-lg bg-[#111113] p-4 text-[13px] leading-[1.6] text-text-secondary">
                    <code>{ep.request}</code>
                  </pre>
                </div>

                <div className="mt-4">
                  <p className="mb-2 text-[11px] font-medium uppercase tracking-[0.15em] text-text-tertiary">Response</p>
                  <pre className="overflow-x-auto rounded-lg bg-[#111113] p-4 text-[13px] leading-[1.6] text-text-secondary">
                    <code>{ep.response}</code>
                  </pre>
                </div>
              </div>
            ))}
          </div>

          <div className="mt-16 rounded-lg border border-border bg-bg-card p-6 text-center">
            <p className="text-[13px] text-text-tertiary">
              Try it now — no API key needed during preview:
            </p>
            <pre className="mt-3 rounded-lg bg-[#18181B] px-4 py-3 text-[14px] text-text-secondary">
              <code>curl -X POST https://api.entity.legal/v1/incorporate</code>
            </pre>
          </div>
        </div>
      </main>

      <Footer />
    </div>
  );
}
