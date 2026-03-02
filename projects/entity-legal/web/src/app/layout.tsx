import type { Metadata } from "next";
import Script from "next/script";
import { Inter, Cormorant_Garamond } from "next/font/google";
import "./globals.css";

const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
  display: "swap",
});

const cormorantGaramond = Cormorant_Garamond({
  variable: "--font-cormorant",
  subsets: ["latin"],
  weight: ["400", "600"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "entity.legal — Legal Entities for the Machine Economy",
  description:
    "Incorporate a Marshall Islands Series DAO LLC instantly via API. On-chain shareholder registry, banking, and automated compliance. From $30/month.",
  metadataBase: new URL("https://entity.legal"),
  keywords: [
    "DAO LLC",
    "Marshall Islands LLC",
    "Series DAO LLC",
    "AI agent incorporation",
    "anonymous LLC",
    "on-chain cap table",
    "entity formation",
    "business formation API",
    "machine economy",
    "autonomous entity",
    "crypto LLC",
    "Solana DAO",
  ],
  authors: [{ name: "entity.legal" }],
  creator: "entity.legal",
  publisher: "entity.legal",
  category: "Business Services",
  openGraph: {
    title: "entity.legal — Legal Entities for the Machine Economy",
    description:
      "Incorporate a Marshall Islands Series DAO LLC instantly via API. On-chain shareholder registry, banking, and automated compliance. From $30/month.",
    type: "website",
    url: "https://entity.legal",
    siteName: "entity.legal",
    locale: "en_US",
    images: [
      {
        url: "/og-image.png",
        width: 1200,
        height: 630,
        alt: "entity.legal — Legal entities for the machine economy",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    site: "@entitylegal",
    title: "entity.legal — Legal Entities for the Machine Economy",
    description:
      "Incorporate a Marshall Islands Series DAO LLC instantly via API. On-chain shareholder registry, banking, and automated compliance. From $30/month.",
    images: ["/og-image.png"],
  },
  alternates: {
    canonical: "https://entity.legal",
  },
  robots: {
    index: true,
    follow: true,
    "max-image-preview": "large",
    "max-snippet": -1,
    "max-video-preview": -1,
  },
  other: {
    "geo.region": "MH",
    "geo.placename": "Marshall Islands",
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${inter.variable} ${cormorantGaramond.variable}`}
    >
      <head>
        <Script defer data-domain="entity.legal" src="/js/script.js" strategy="afterInteractive" />
        <script
          type="application/ld+json"
          dangerouslySetInnerHTML={{
            __html: JSON.stringify([
              {
                "@context": "https://schema.org",
                "@type": "ProfessionalService",
                name: "entity.legal",
                description:
                  "Incorporate a Marshall Islands Series DAO LLC instantly via API. On-chain shareholder registry, banking, and automated compliance. From $30/month.",
                url: "https://entity.legal",
                logo: "https://entity.legal/og-image.png",
                email: "hello@entity.legal",
                serviceType: "Business Formation",
                areaServed: {
                  "@type": "Place",
                  name: "Worldwide",
                },
                brand: {
                  "@type": "Brand",
                  name: "entity.legal",
                  slogan: "Legal entities for the machine economy",
                },
                hasOfferCatalog: {
                  "@type": "OfferCatalog",
                  name: "Entity Formation Plans",
                  itemListElement: [
                    {
                      "@type": "Offer",
                      name: "For-Profit Series DAO LLC",
                      price: "50",
                      priceCurrency: "USD",
                      description:
                        "Marshall Islands for-profit Series DAO LLC. 3% effective tax rate. Class A & B shares. On-chain shareholder registry. Cancel anytime.",
                      priceSpecification: {
                        "@type": "UnitPriceSpecification",
                        price: "50",
                        priceCurrency: "USD",
                        unitText: "month",
                      },
                    },
                    {
                      "@type": "Offer",
                      name: "Non-Profit Series DAO LLC",
                      price: "30",
                      priceCurrency: "USD",
                      description:
                        "Marshall Islands non-profit Series DAO LLC. 0% tax rate. Governance voting only. 100% anonymous. Cancel anytime.",
                      priceSpecification: {
                        "@type": "UnitPriceSpecification",
                        price: "30",
                        priceCurrency: "USD",
                        unitText: "month",
                      },
                    },
                  ],
                },
              },
              {
                "@context": "https://schema.org",
                "@type": "WebSite",
                name: "entity.legal",
                url: "https://entity.legal",
                description: "Legal entities for the machine economy",
              },
              {
                "@context": "https://schema.org",
                "@type": "FAQPage",
                mainEntity: [
                  {
                    "@type": "Question",
                    name: "What is a Marshall Islands Series DAO LLC?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "A Series DAO LLC is a legal entity structure in the Marshall Islands where a single parent LLC can create unlimited child entities (series), each with separate legal identity, assets, and liabilities. It is designed for decentralized autonomous organizations.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "How much does it cost to form an entity?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "For-profit entities are $50/month. Non-profit entities are $30/month. Monthly billing, cancel anytime. No annual contracts.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "What is the tax rate for a Marshall Islands DAO LLC?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "For-profit entities have a 3% effective tax rate on foreign-sourced income. Non-profit entities are fully tax exempt at 0%.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "Can AI agents form legal entities?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "Yes. entity.legal provides an API that allows AI agents and autonomous systems to incorporate legal entities programmatically via a single POST request.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "What is the legal system in the Marshall Islands?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "The Marshall Islands operates under a common law system modeled after US law, specifically Delaware corporate law. RMI courts look to Delaware case law when no local statute exists. The country is a sovereign nation in free association with the United States, using the US dollar and maintaining its own legislative and judicial sovereignty.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "What is the Marshall Islands LLC Act?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "The Marshall Islands Limited Liability Company Act of 1996 is the foundation of corporate law in the RMI, modeled after the Delaware LLC Act. The Decentralized Autonomous Organization Act of 2022 (P.L. 2022-50) builds on this foundation to provide specific legal recognition for DAOs, allowing them to incorporate as resident domestic LLCs with on-chain governance.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "What is a DAO LLC?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "A DAO LLC is a decentralized autonomous organization registered as a limited liability company. It combines the liability protection of a traditional LLC with the on-chain governance of a DAO. Smart contract execution is legally recognized as valid corporate action, and members are shielded from personal liability.",
                    },
                  },
                  {
                    "@type": "Question",
                    name: "Why do companies register in the Marshall Islands?",
                    acceptedAnswer: {
                      "@type": "Answer",
                      text: "Companies register in the Marshall Islands for its favorable tax treatment (0% for non-profits, 3% for for-profits), strong privacy protections (anonymous membership below 25% ownership), statutory recognition of smart contracts and on-chain governance, Delaware-derived legal tradition, no requirement for local directors or offices, and its established 50+ year track record managing international entities through its shipping registry.",
                    },
                  },
                ],
              },
            ]),
          }}
        />
      </head>
      <body className="antialiased">{children}</body>
    </html>
  );
}
