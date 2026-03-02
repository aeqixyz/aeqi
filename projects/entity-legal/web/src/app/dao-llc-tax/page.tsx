import type { Metadata } from "next";
import Link from "next/link";
import { Article } from "@/components/Article";
import { ArticleSchema } from "@/components/ArticleSchema";

export const metadata: Metadata = {
  title: "DAO LLC Tax Structure — For-Profit vs. Non-Profit | entity.legal",
  description:
    "Complete guide to Marshall Islands DAO LLC taxation. 3% Gross Revenue Tax for for-profit entities, 0% for non-profits, territorial exemptions, and distribution rules.",
  alternates: { canonical: "https://entity.legal/dao-llc-tax" },
  openGraph: {
    title: "DAO LLC Tax Structure — For-Profit vs. Non-Profit",
    description:
      "Complete guide to Marshall Islands DAO LLC taxation. 3% GRT for for-profit, 0% for non-profits, territorial exemptions.",
    url: "https://entity.legal/dao-llc-tax",
  },
};

export default function DAOLLCTaxPage() {
  return (
    <>
    <ArticleSchema
      title="DAO LLC Tax Structure — For-Profit vs. Non-Profit"
      description="Complete guide to Marshall Islands DAO LLC taxation. 3% Gross Revenue Tax for for-profit entities, 0% for non-profits, territorial exemptions, and distribution rules."
      url="https://entity.legal/dao-llc-tax"
      publishedDate="2026-02-28"
      updatedDate="2026-02-28"
      breadcrumbs={[
        { name: "Home", url: "https://entity.legal" },
        { name: "Learn", url: "https://entity.legal/learn" },
        { name: "DAO LLC Tax Structure", url: "https://entity.legal/dao-llc-tax" },
      ]}
    />
    <Article
      title="DAO LLC Tax Structure"
      subtitle="For-profit vs. non-profit election, the 3% Gross Revenue Tax, territorial exemptions, and the 0% non-profit treatment."
      publishedDate="February 2026"
      updatedDate="February 2026"
      slug="dao-llc-tax"
    >
      <p>
        The RMI DAO framework is intentionally agnostic regarding the organization&rsquo;s purpose, allowing registration as either a <strong>for-profit</strong> or <strong>non-profit</strong> entity. This election is fundamental to the DAO&rsquo;s tax obligations, internal distribution rules, and governance structure.
      </p>

      <hr />

      <h2>Side-by-Side Comparison</h2>
      <table>
        <thead>
          <tr>
            <th>Feature</th>
            <th>For-Profit</th>
            <th>Non-Profit</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Tax Rate</td>
            <td>3% GRT on gross revenue</td>
            <td>0% — fully tax exempt</td>
          </tr>
          <tr>
            <td>Capital Gains Tax</td>
            <td>0%</td>
            <td>0%</td>
          </tr>
          <tr>
            <td>Withholding Tax</td>
            <td>0%</td>
            <td>0%</td>
          </tr>
          <tr>
            <td>Profit Distribution</td>
            <td>Permitted to members</td>
            <td>Prohibited</td>
          </tr>
          <tr>
            <td>Ownership</td>
            <td>Beneficial owners</td>
            <td>Beneficial members (no ownership)</td>
          </tr>
          <tr>
            <td>Revenue Reporting</td>
            <td>Required annually</td>
            <td>Not required</td>
          </tr>
          <tr>
            <td>Foreign Source Income</td>
            <td>Generally untaxed (territorial)</td>
            <td>N/A — all exempt</td>
          </tr>
          <tr>
            <td>Share Structure</td>
            <td>Class A (Voting) + Class B (Voting + Profit)</td>
            <td>Governance tokens (Voting only)</td>
          </tr>
        </tbody>
      </table>

      <h2>The For-Profit DAO LLC</h2>
      <p>
        For organizations intended to generate revenue and distribute earnings — investment DAOs, DeFi protocols with fee-sharing, NFT marketplaces — the for-profit status provides the legal path to return value to members.
      </p>

      <h3>The 3% Gross Revenue Tax</h3>
      <p>
        For-profit DAO LLCs pay a <strong>3% tax on gross revenue</strong> as defined in Section 109 of the Income Tax Act 1989. Key details:
      </p>
      <ul>
        <li><strong>Territorial system</strong> — Revenue generated from foreign sources is generally untaxed. For most DAOs operating globally, the effective tax burden is minimal.</li>
        <li><strong>Capital gains excluded</strong> — Capital gains are explicitly excluded from the GRT tax base. Token appreciation, asset sales, and investment returns are not subject to the 3% tax.</li>
        <li><strong>Dividends excluded</strong> — Dividend income received by the entity is also excluded from the GRT calculation. This is significant for DAOs holding equity positions in other entities or receiving distributions from Series sub-entities.</li>
        <li><strong>Annual filing</strong> — Revenue must be reported and the tax paid annually</li>
        <li><strong>Distribution rights</strong> — For-profit DAOs may distribute earnings among members based on the rules in their operating agreement or smart contracts</li>
      </ul>
      <p>
        The combined effect of these exclusions means the 3% GRT applies only to <strong>active business revenue</strong> — fees, service income, and similar operational receipts. For DAOs whose primary activity is holding and trading assets rather than generating service revenue, the effective tax rate approaches zero.
      </p>

      <h3>Share Classes</h3>
      <p>
        entity.legal structures for-profit entities with two share classes:
      </p>
      <ul>
        <li><strong>Class A — Voting</strong> — Governance rights only. 100% anonymous. No profit distribution rights. Ideal for contributors, advisors, and governance participants who don&rsquo;t need economic exposure.</li>
        <li><strong>Class B — Voting + Profit</strong> — Full governance rights plus profit distribution. Shareholders receive proportional distributions from the entity&rsquo;s earnings.</li>
      </ul>
      <p>
        Shares can be swapped between Class A and Class B at any time, allowing members to move between governance-only and profit-sharing roles as the organization evolves.
      </p>

      <h2>The Non-Profit DAO LLC</h2>
      <p>
        Non-profit DAO LLCs are governed by both the DAO Act and the <strong>Non-Profit Entities Act of 2020</strong>. They are designed for public goods, grant-making, social impact, and protocol development.
      </p>

      <h3>Defining Characteristics</h3>
      <ul>
        <li><strong>No distributions</strong> — Strictly prohibited from distributing any funds or property among members</li>
        <li><strong>Ownerless structure</strong> — Members have &ldquo;beneficial membership&rdquo; (responsibility and voting) but no ownership interest or economic rights</li>
        <li><strong>Complete tax neutrality</strong> — 0% corporate income, capital gains, and withholding taxes</li>
        <li><strong>No revenue reporting</strong> — Non-profits are not required to report their revenues annually</li>
      </ul>

      <h3>Ideal Use Cases</h3>
      <ul>
        <li><strong>Protocol treasuries</strong> — Managing community funds where the goal is to fund development, not return profits</li>
        <li><strong>Developer grants</strong> — Distributing funding for open-source development</li>
        <li><strong>Public goods</strong> — Any mission-driven DAO where profit distribution would undermine the purpose</li>
        <li><strong>Research organizations</strong> — Academic or scientific DAOs with community governance</li>
      </ul>

      <hr />

      <h2>Choosing Your Election</h2>
      <p>
        The choice between for-profit and non-profit is irrevocable at formation and determines the fundamental character of the entity. The key question: <strong>will the entity ever distribute profits to members?</strong>
      </p>
      <p>
        If yes — for-profit. The 3% GRT is one of the lowest effective tax rates globally, and the territorial system means most internationally-sourced revenue faces no tax at all.
      </p>
      <p>
        If no — non-profit. The 0% rate and absence of reporting obligations make it the cleanest structure for mission-driven organizations.
      </p>

      <h2>Further Reading</h2>
      <ul>
        <li><Link href="/marshall-islands-dao-llc">Marshall Islands DAO LLC</Link> — The complete legislative framework</li>
        <li><Link href="/series-dao-llc">Series DAO LLC</Link> — Segregated liability for multi-product DAOs</li>
        <li><Link href="/dao-llc-compliance">Compliance & KYC</Link> — BOIR, FIBL, and reporting requirements</li>
        <li><Link href="/dao-llc-vs-wyoming">Jurisdiction Comparison</Link> — Tax treatment across jurisdictions</li>
      </ul>
    </Article>
    </>
  );
}
