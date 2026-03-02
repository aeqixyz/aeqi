import type { Metadata } from "next";
import Link from "next/link";
import { Footer } from "@/components/Footer";

export const metadata: Metadata = {
  title: "Privacy Policy — entity.legal",
};

export default function Privacy() {
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
          <span className="text-text-muted">Privacy Policy</span>
        </div>
      </nav>

      <main className="px-6 py-16 md:py-24">
        <div className="mx-auto max-w-[600px]">
          <h1 className="font-serif text-3xl text-text-primary">Privacy Policy</h1>
          <p className="mt-2 text-[13px] text-text-tertiary">Last updated: February 2026</p>

          <div className="mt-10 space-y-8 text-[14px] leading-[1.8] text-text-secondary">
            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">What we collect</h2>
              <p>When you join our waitlist, we collect your email address. We do not collect names, phone numbers, payment information, or any other personal data at this stage.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">How we use it</h2>
              <p>Your email address is used solely to notify you when entity.legal launches and to send occasional product updates. We will never sell, rent, or share your email with third parties.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Data storage</h2>
              <p>Email addresses are stored on our servers located in the European Union. We use industry-standard security measures to protect your data.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Your rights</h2>
              <p>You may request deletion of your data at any time by emailing <a href="mailto:hello@entity.legal" className="text-text-primary underline underline-offset-2">hello@entity.legal</a>. We will remove your information within 48 hours.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Cookies</h2>
              <p>We do not use cookies, tracking pixels, or analytics scripts. This site does not track you.</p>
            </section>

            <section>
              <h2 className="mb-3 text-[15px] font-medium text-text-primary">Contact</h2>
              <p>For privacy-related inquiries, contact <a href="mailto:hello@entity.legal" className="text-text-primary underline underline-offset-2">hello@entity.legal</a>.</p>
            </section>
          </div>
        </div>
      </main>

      <Footer />
    </div>
  );
}
