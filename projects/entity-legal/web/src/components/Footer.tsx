import Link from "next/link";

const learnLinks = [
  { href: "/marshall-islands-dao-llc", label: "Marshall Islands DAO LLC" },
  { href: "/series-dao-llc", label: "Series DAO LLC" },
  { href: "/dao-llc-tax", label: "DAO LLC Tax Structure" },
  { href: "/dao-llc-compliance", label: "Compliance & KYC" },
  { href: "/dao-llc-vs-wyoming", label: "RMI vs. Wyoming vs. Cayman" },
  { href: "/ai-agent-legal-entity", label: "AI Agent Incorporation" },
  { href: "/dao-llc-banking", label: "DAO LLC Banking" },
  { href: "/offshore-dao-myths", label: "Offshore DAO Myths" },
];

export function Footer() {
  return (
    <footer className="border-t border-border bg-bg-primary px-6 py-12 md:py-16">
      <div className="mx-auto max-w-[720px]">
        <div className="grid grid-cols-2 gap-8 md:grid-cols-3 md:gap-12">
          {/* Product */}
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.2em] text-text-tertiary">
              Product
            </p>
            <ul className="mt-4 space-y-2.5">
              <li>
                <Link href="/docs" className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                  API Reference
                </Link>
              </li>
              <li>
                <Link href="/learn" className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                  Knowledge Base
                </Link>
              </li>
              <li>
                <a href="mailto:hello@entity.legal" className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                  Contact
                </a>
              </li>
            </ul>
          </div>

          {/* Learn */}
          <div className="md:col-span-1">
            <p className="text-[11px] font-medium uppercase tracking-[0.2em] text-text-tertiary">
              Learn
            </p>
            <ul className="mt-4 space-y-2.5">
              {learnLinks.map((link) => (
                <li key={link.href}>
                  <Link href={link.href} className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                    {link.label}
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          {/* Ecosystem */}
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.2em] text-text-tertiary">
              Ecosystem
            </p>
            <ul className="mt-4 space-y-2.5">
              <li>
                <a href="https://entity.directory" className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                  Entity Directory
                </a>
              </li>
            </ul>
            <p className="mt-6 text-[11px] font-medium uppercase tracking-[0.2em] text-text-tertiary">
              Legal
            </p>
            <ul className="mt-4 space-y-2.5">
              <li>
                <Link href="/privacy" className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                  Privacy Policy
                </Link>
              </li>
              <li>
                <Link href="/terms" className="text-[13px] text-text-secondary transition-colors hover:text-text-primary">
                  Terms of Service
                </Link>
              </li>
            </ul>
          </div>
        </div>

        {/* Bottom bar */}
        <div className="mt-12 flex items-center justify-between border-t border-border pt-6">
          <Link href="/" className="font-serif text-[14px] tracking-[0.12em] text-text-secondary transition-colors hover:text-text-primary">
            entity<span className="text-text-tertiary">.</span>legal
          </Link>
          <span className="text-[12px] text-text-muted">
            &copy; {new Date().getFullYear()} entity.legal
          </span>
        </div>
      </div>
    </footer>
  );
}
