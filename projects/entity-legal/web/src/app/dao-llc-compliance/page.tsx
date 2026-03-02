import type { Metadata } from "next";
import Link from "next/link";
import { Article } from "@/components/Article";
import { ArticleSchema } from "@/components/ArticleSchema";

export const metadata: Metadata = {
  title: "DAO LLC Compliance & KYC Requirements | entity.legal",
  description:
    "KYC thresholds, Beneficial Owner Information Reports, Foreign Investment Business License, on-chain monitoring, and the three-tiered agency system for Marshall Islands DAO LLCs.",
  alternates: { canonical: "https://entity.legal/dao-llc-compliance" },
  openGraph: {
    title: "DAO LLC Compliance & KYC Requirements",
    description:
      "KYC thresholds, BOIR, FIBL, on-chain monitoring, and the three-tiered agency system for Marshall Islands DAO LLCs.",
    url: "https://entity.legal/dao-llc-compliance",
  },
};

export default function DAOLLCCompliancePage() {
  return (
    <>
    <ArticleSchema
      title="DAO LLC Compliance & KYC Requirements"
      description="KYC thresholds, Beneficial Owner Information Reports, Foreign Investment Business License, on-chain monitoring, and the three-tiered agency system for Marshall Islands DAO LLCs."
      url="https://entity.legal/dao-llc-compliance"
      publishedDate="2026-02-28"
      updatedDate="2026-02-28"
      breadcrumbs={[
        { name: "Home", url: "https://entity.legal" },
        { name: "Learn", url: "https://entity.legal/learn" },
        { name: "DAO LLC Compliance & KYC", url: "https://entity.legal/dao-llc-compliance" },
      ]}
    />
    <Article
      title="DAO LLC Compliance & KYC"
      subtitle="Beneficial Owner Information Reports, KYC thresholds, the Foreign Investment Business License, and the three-tiered agency system."
      publishedDate="February 2026"
      updatedDate="February 2026"
      slug="dao-llc-compliance"
    >
      <p>
        The Marshall Islands has adopted a <strong>proportional approach to regulation</strong> — meeting international AML/KYC standards without stifling the innovation inherent in decentralized protocols. The 2024 Regulations clarify identification requirements, monitoring obligations, and the roles of various agents.
      </p>

      <hr />

      <h2>KYC Thresholds</h2>
      <table>
        <thead>
          <tr>
            <th>Requirement</th>
            <th>Threshold</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>KYC Screening</td>
            <td>Mandatory for any member holding 25% or more of governance or voting rights</td>
          </tr>
          <tr>
            <td>Managing Members</td>
            <td>All managers or officers (if appointed) must undergo KYC</td>
          </tr>
          <tr>
            <td>Required Documents</td>
            <td>Proof of identity (passport) and proof of residential address</td>
          </tr>
          <tr>
            <td>Below 25%</td>
            <td>Membership can remain fully anonymous</td>
          </tr>
        </tbody>
      </table>
      <p>
        For the majority of community members — those holding less than 25% of tokens — <strong>membership remains anonymous</strong>. This is a critical feature that allows permissionless token trading and participation while ensuring that Ultimate Beneficial Owners (UBOs) are known to the registry.
      </p>

      <h2>Beneficial Owner Information Report (BOIR)</h2>
      <p>
        Every DAO LLC must file a <strong>BOIR annually between January 1st and March 31st</strong>. This report identifies any member holding 25% or more of governance rights and any appointed managers or officers. The report is filed with the Registered Agent, not made public.
      </p>

      <h2>On-Chain Monitoring</h2>
      <p>
        The 2024 Regulations authorize law enforcement and the Registered Agent to <strong>monitor the blockchain(s) used by the DAO LLC</strong>. This is specifically to ensure the organization is not involved in money laundering or other illegal activities.
      </p>
      <p>
        If a Registered Agent has &ldquo;reasonable grounds&rdquo; to suspect illicit activity, they are legally obligated to report it to the Registrar or relevant law enforcement.
      </p>

      <h2>The Three-Tiered Agency System</h2>
      <p>
        Every RMI DAO LLC must navigate a three-tiered system of agency to maintain its legal presence:
      </p>

      <h3>1. Registered Agent</h3>
      <p>
        A local entity that maintains the physical office in the RMI and receives service of process. The Registered Agent is the DAO&rsquo;s legal anchor in the jurisdiction — without one, the entity cannot exist.
      </p>

      <h3>2. Representative Agent</h3>
      <p>
        A person or persons designated by the DAO to act as a point of contact. If the representative is no longer a member or is unable to represent the DAO, the Registered Agent steps in until a new representative is appointed. This role bridges the gap between the on-chain DAO and off-chain legal requirements.
      </p>

      <h3>3. Nominee Services</h3>
      <p>
        For DAOs seeking enhanced privacy, <strong>nominee services are legally available</strong>. Organizations can act as the &ldquo;managing member&rdquo; for KYC purposes, allowing the actual founders to remain anonymous in public records. This is not evasion — it is a legally sanctioned privacy mechanism.
      </p>

      <h2>Foreign Investment Business License (FIBL)</h2>
      <p>
        Because DAO LLCs are technically &ldquo;non-citizens&rdquo; if owned by foreigners, they must obtain a <strong>Foreign Investment Business License</strong> if conducting business in the RMI.
      </p>

      <h3>The Reserved List</h3>
      <p>
        Certain sectors are restricted to Marshallese citizens:
      </p>
      <ul>
        <li>Small-scale retail (turnover under $50,000)</li>
        <li>Small agriculture</li>
        <li>Water-taxi services</li>
      </ul>
      <p>
        Most DAO activities, being digital and global, <strong>do not fall under these restrictions</strong>.
      </p>

      <h3>FIBL Process</h3>
      <ul>
        <li><strong>Application fee</strong> — $250</li>
        <li><strong>Processing time</strong> — 30 days or less (recently streamlined by OCIT)</li>
        <li><strong>Compliance duty</strong> — All FIBL holders must maintain reliable accounting and ownership records</li>
      </ul>
      <p>
        For most offshore DAOs, the FIBL is a formality that certifies they are not competing in the local Marshallese retail market.
      </p>

      <hr />

      <h2>The entity.legal Approach</h2>
      <p>
        entity.legal handles the full compliance stack automatically. When you incorporate through our platform:
      </p>
      <ul>
        <li><strong>Registered Agent</strong> — Maintained by entity.legal in the Marshall Islands</li>
        <li><strong>Representative Agent</strong> — Designated according to your governance structure</li>
        <li><strong>BOIR Filing</strong> — Automated annual filing between January and March</li>
        <li><strong>FIBL</strong> — Processed as part of your formation package</li>
        <li><strong>KYC Screening</strong> — Streamlined for qualifying members; anonymous membership preserved for all others</li>
      </ul>
      <p>
        Compliance should not be a barrier to formation. It should be invisible infrastructure.
      </p>

      <h2>Further Reading</h2>
      <ul>
        <li><Link href="/marshall-islands-dao-llc">Marshall Islands DAO LLC</Link> — The complete legislative framework</li>
        <li><Link href="/dao-llc-tax">DAO LLC Tax Structure</Link> — For-profit (3% GRT) vs. non-profit (0%)</li>
        <li><Link href="/series-dao-llc">Series DAO LLC</Link> — Segregated liability and sub-DAOs</li>
        <li><Link href="/dao-llc-vs-wyoming">Jurisdiction Comparison</Link> — Compliance across jurisdictions</li>
      </ul>
    </Article>
    </>
  );
}
