import type { Metadata } from "next";
import Link from "next/link";
import { Article } from "@/components/Article";
import { ArticleSchema } from "@/components/ArticleSchema";

export const metadata: Metadata = {
  title: "Series DAO LLC — Sub-DAOs and Segregated Liability | entity.legal",
  description:
    "How the Marshall Islands Series LLC structure enables segregated assets, ring-fenced liability, and unlimited child entities under a single parent DAO LLC.",
  alternates: { canonical: "https://entity.legal/series-dao-llc" },
  openGraph: {
    title: "Series DAO LLC — Sub-DAOs and Segregated Liability",
    description:
      "How the Series LLC structure enables segregated assets, ring-fenced liability, and unlimited child entities under a single parent.",
    url: "https://entity.legal/series-dao-llc",
  },
};

export default function SeriesDAOLLCPage() {
  return (
    <>
    <ArticleSchema
      title="Series DAO LLC — Sub-DAOs and Segregated Liability"
      description="How the Marshall Islands Series LLC structure enables segregated assets, ring-fenced liability, and unlimited child entities under a single parent DAO LLC."
      url="https://entity.legal/series-dao-llc"
      publishedDate="2026-02-28"
      updatedDate="2026-02-28"
      breadcrumbs={[
        { name: "Home", url: "https://entity.legal" },
        { name: "Learn", url: "https://entity.legal/learn" },
        { name: "Series DAO LLC", url: "https://entity.legal/series-dao-llc" },
      ]}
    />
    <Article
      title="Series DAO LLC"
      subtitle="How the Series LLC structure enables segregated assets, ring-fenced liability, and multi-protocol ecosystems under a single parent entity."
      publishedDate="February 2026"
      updatedDate="February 2026"
      slug="series-dao-llc"
    >
      <p>
        The Series LLC concept, based on the Delaware model and introduced to the Marshall Islands through the <strong>2023 Amendment Act</strong>, is perhaps the most powerful tool for protocol scaling within the RMI framework. This structure allows a single Master LLC to create multiple distinct series, each functioning as an independent unit with its own assets, liabilities, members, and governance.
      </p>

      <hr />

      <h2>How Series Segregation Works</h2>
      <p>
        In a Series DAO LLC, each individual series is <strong>isolated from the liability exposure of every other series</strong>. This &ldquo;ring-fencing&rdquo; is essential for DAOs managing multiple high-risk projects or diverse portfolios.
      </p>
      <p>
        For example, a DAO might establish:
      </p>
      <ul>
        <li>One series for a <strong>lending protocol</strong></li>
        <li>Another for a <strong>venture treasury</strong></li>
        <li>A third for a <strong>real-world asset (RWA) pool</strong></li>
      </ul>
      <p>
        If the lending protocol series incurs significant debts or legal claims, the assets of the venture treasury and the RWA pool <strong>remain protected</strong> from the lending series&rsquo; creditors. Each series is a separate legal silo.
      </p>

      <h2>Series vs. Protected Cell Companies</h2>
      <p>
        The Series LLC structure is superior to &ldquo;protected cell companies&rdquo; (PCCs) found in jurisdictions like Guernsey, Malta, and the Cayman Islands. While PCCs offer some isolation, they carry restrictive corporate governance requirements and limitations on how cells can interact. The Series LLC provides <strong>robust isolation without these constraints</strong>.
      </p>

      <h2>Administrative Advantages</h2>
      <p>
        The Series structure offers significant operational efficiency:
      </p>
      <table>
        <thead>
          <tr>
            <th>Feature</th>
            <th>Detail</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Consolidated Reporting</td>
            <td>No additional reporting requirements for individual series beyond the Master DAO LLC</td>
          </tr>
          <tr>
            <td>Flexible Membership</td>
            <td>Each series can have different members and managers</td>
          </tr>
          <tr>
            <td>Cost Savings</td>
            <td>One set of annual fees and a single registered agent for unlimited series</td>
          </tr>
          <tr>
            <td>Instant Formation</td>
            <td>New series can be spawned programmatically without separate government filings</td>
          </tr>
        </tbody>
      </table>

      <h2>Maintaining Series Protection</h2>
      <p>
        To maintain the liability isolation between series, the law requires that the assets and operations of each series be kept distinct. Best practices include:
      </p>
      <ul>
        <li><strong>Separate wallets</strong> — Maintain separate bank accounts or crypto wallets for each series</li>
        <li><strong>Distinct contracts</strong> — All contracts must be signed specifically in the name of the relevant series, not the parent</li>
        <li><strong>Clear records</strong> — Bookkeeping must clearly delineate which assets belong to which series</li>
        <li><strong>No commingling</strong> — Assets and revenues of different series should never be mixed</li>
      </ul>

      <h2>Use Cases</h2>

      <h3>DeFi Protocols</h3>
      <p>
        A DeFi protocol can isolate each product line — lending, swaps, yield aggregation — into its own series. A vulnerability or legal action against one product does not endanger the treasury or users of other products.
      </p>

      <h3>Investment DAOs</h3>
      <p>
        A venture DAO can create a new series for each investment, allowing different investor compositions and return profiles per deal while sharing a single parent entity for governance and administration.
      </p>

      <h3>Real-World Assets</h3>
      <p>
        Each physical asset — real estate, intellectual property, equipment — can be held in its own series, providing clean bankruptcy remoteness and simplified transfer or liquidation.
      </p>

      <h3>AI Agent Networks</h3>
      <p>
        An autonomous AI system can spawn child entities on demand — each series operating its own wallet, signing its own contracts, and maintaining its own liability boundary. The parent entity provides governance and oversight while each agent operates independently.
      </p>

      <hr />

      <h2>The entity.legal Approach</h2>
      <p>
        entity.legal leverages the Series DAO LLC structure to enable <strong>instant child entity formation</strong>. When you incorporate through our API, you receive a parent Series DAO LLC. From there, new series can be created programmatically — each with its own on-chain shareholder registry, bank account, and compliance wrapper.
      </p>
      <p>
        This is the architecture that makes &ldquo;legal entities for the machine economy&rdquo; possible. One API call. One new entity. Fully isolated. Fully sovereign.
      </p>

      <h2>Further Reading</h2>
      <ul>
        <li><Link href="/marshall-islands-dao-llc">Marshall Islands DAO LLC</Link> — The complete legislative framework</li>
        <li><Link href="/dao-llc-tax">DAO LLC Tax Structure</Link> — For-profit vs. non-profit taxation</li>
        <li><Link href="/dao-llc-compliance">Compliance & KYC</Link> — Reporting and identification requirements</li>
        <li><Link href="/ai-agent-legal-entity">Legal Entities for AI Agents</Link> — Machine incorporation via Series LLC</li>
      </ul>
    </Article>
    </>
  );
}
