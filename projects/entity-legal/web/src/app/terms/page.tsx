import type { Metadata } from "next";
import Link from "next/link";
import { Footer } from "@/components/Footer";

export const metadata: Metadata = {
  title: "Terms of Service — entity.legal",
};

export default function Terms() {
  return (
    <div className="min-h-screen bg-bg-primary">
      <header className="border-b border-border px-6 py-4">
        <div className="mx-auto flex max-w-[600px] items-center justify-between">
          <Link href="/" className="font-serif text-[14px] tracking-[0.12em] text-text-secondary transition-colors hover:text-text-primary">
            entity<span className="text-text-tertiary">.</span>legal
          </Link>
        </div>
      </header>

      <nav className="px-6 pt-6" aria-label="Breadcrumb">
        <div className="mx-auto max-w-[600px] flex items-center gap-2 text-[12px] text-text-tertiary">
          <Link href="/" className="transition-colors hover:text-text-secondary">Home</Link>
          <span>/</span>
          <span className="text-text-muted">Terms of Service</span>
        </div>
      </nav>

      <main className="px-6 py-16 md:py-24">
        <div className="mx-auto max-w-[600px]">
          <h1 className="font-serif text-3xl text-text-primary">Terms of Service</h1>
          <p className="mt-2 text-[13px] text-text-tertiary">Last updated: February 2026</p>

          <div className="mt-10 space-y-8 text-[14px] leading-[1.8] text-text-secondary">
            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Service</h2>
              <p>entity.legal provides legal entity formation services in the Republic of the Marshall Islands. We facilitate the creation of Series DAO LLCs and related corporate structures through our platform and API.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Waitlist</h2>
              <p>The service is currently in pre-launch. By joining the waitlist, you are expressing interest in the service. No payment is required and no obligation is created. Waitlist position does not guarantee access.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Not legal advice</h2>
              <p>entity.legal is a formation service, not a law firm. Nothing on this website constitutes legal, tax, or financial advice. Consult qualified professionals for advice specific to your situation and jurisdiction.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Limitation of liability</h2>
              <p>The service is provided &ldquo;as is&rdquo; without warranties of any kind. entity.legal shall not be liable for any indirect, incidental, or consequential damages arising from use of the service.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Governing law</h2>
              <p>These terms are governed by the laws of the Republic of the Marshall Islands.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Contact</h2>
              <p>Questions about these terms? Email <a href="mailto:hello@entity.legal" className="text-text-primary underline underline-offset-2">hello@entity.legal</a>.</p>
            </section>
          </div>
        </div>
      </main>

      <Footer />
    </div>
  );
}
