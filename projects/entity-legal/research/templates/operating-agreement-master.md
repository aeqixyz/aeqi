# MASTER OPERATING AGREEMENT OF [ENTITY NAME] DAO LLC

## A Series Decentralized Autonomous Organization Limited Liability Company Organized Under the Laws of the Republic of the Marshall Islands

**Effective Date**: [DATE]

**Registration Number**: [REGISTRATION NUMBER]

**Entity Type**: [FOR-PROFIT / NON-PROFIT]

---

This Master Operating Agreement (this "Master Agreement") of [ENTITY NAME] DAO LLC (the "Company"), a [for-profit / non-profit] decentralized autonomous organization limited liability company organized as a Series LLC under the laws of the Republic of the Marshall Islands, is entered into and effective as of [DATE] (the "Effective Date"), by and among the Members (as defined herein) of the Company.

WHEREAS, the Company was formed as a [for-profit / non-profit] DAO LLC pursuant to the Decentralized Autonomous Organization Act of 2022 (Public Law 2022-50), as amended by the 2023 Amendments and the 2024 Regulations (collectively, the "DAO Act"), and the Revised Uniform Limited Liability Company Act (Title 52 MIRC Chapter 4) (the "LLC Act") of the Republic of the Marshall Islands;

WHEREAS, the Company is structured as a Series LLC under the Series DAO LLC provisions introduced by the 2023 Amendments to the DAO Act, and this Master Agreement governs the Company as a whole, with each Series governed by a Series Operating Supplement that incorporates and is subject to this Master Agreement;

WHEREAS, the Company utilizes a two-token structure consisting of Security Tokens and Utility Tokens deployed on the Solana blockchain, with the smart contract serving as the authoritative membership registry as recognized by the DAO Act;

WHEREAS, [FOUNDATION NAME] (the "Foundation"), a non-profit DAO LLC organized under the laws of the Republic of the Marshall Islands, has been designated as the Special Delegate of the Company to perform limited administrative, compliance, and representative functions;

WHEREAS, this Master Agreement establishes the framework under which all Series of the Company shall operate, and each Series Operating Supplement shall be read together with and subject to this Master Agreement;

NOW, THEREFORE, in consideration of the mutual covenants and agreements set forth herein, and for other good and valuable consideration, the receipt and sufficiency of which are hereby acknowledged, the Members agree as follows:

---

## PART ONE: MASTER LLC PROVISIONS

---

## ARTICLE I: ORGANIZATION OF THE MASTER LLC

### Section 1.1 — Name

The name of the Company is [ENTITY NAME] DAO LLC. The Company may conduct business under such name or any other name approved by the Members through the Governance Program.

### Section 1.2 — Entity Type and Designation

The Company is a [for-profit / non-profit] Decentralized Autonomous Organization Limited Liability Company organized as a Series LLC. The Company is designated as a "[MEMBER-MANAGED / ALGORITHMICALLY MANAGED]" DAO LLC under the DAO Act. The Company serves as the master LLC under which one or more Series may be established, each with separate assets, liabilities, governance, and membership.

### Section 1.3 — Formation

The Company was formed on [FORMATION DATE] upon the filing of the Certificate of Formation with the Registrar of Corporations of the Republic of the Marshall Islands and the issuance of the Certificate of Formation by the Registrar.

### Section 1.4 — Governing Law

This Master Agreement and the rights, obligations, and liabilities of the Members shall be governed by and construed in accordance with the laws of the Republic of the Marshall Islands, including the DAO Act and the LLC Act. Where the DAO Act and LLC Act are silent, the Company follows Delaware LLC precedent as incorporated by reference in RMI law, except where conflicting RMI precedent or law exists.

### Section 1.5 — Legal Hierarchy

In the event of any conflict between the sources of authority governing the Company or any Series, the following hierarchy of legal application shall apply, with the highest-listed authority prevailing:

(a) The DAO Act and applicable law of the Republic of the Marshall Islands;

(b) The LLC Act and other applicable conventional LLC law of the Republic of the Marshall Islands;

(c) This written Master Agreement;

(d) The applicable Series Operating Supplement;

(e) The Smart Contract Code (as defined in Article II).

### Section 1.6 — Registered Agent and Office

The registered agent of the Company is [REGISTERED AGENT NAME], with a registered office at [REGISTERED OFFICE ADDRESS], Republic of the Marshall Islands. The Company shall continuously maintain a registered agent and registered office within the Republic of the Marshall Islands as required by the LLC Act. The registered agent serves the Company and all Series.

### Section 1.7 — Purpose

The Company is organized as a master Series LLC for the purpose of establishing and administering one or more Series, each of which shall pursue purposes as defined in its Series Operating Supplement. The Company itself may hold assets, enter into contracts, and conduct activities in furtherance of the administration of the Series structure, including but not limited to shared infrastructure, compliance, governance, and treasury management. [IF NON-PROFIT: All activities of the Company and its Series shall be consistent with the non-profit restrictions set forth in Article VIII of this Master Agreement.]

### Section 1.8 — Duration

The Company shall have perpetual existence unless dissolved in accordance with Article XII of this Master Agreement.

### Section 1.9 — Foreign Investment Business License

The Company has obtained or shall obtain a Foreign Investment Business License ("FIBL") as required by the laws of the Republic of the Marshall Islands for entities engaging in business in the jurisdiction.

### Section 1.10 — Relationship Between Master LLC and Series

The Company serves as the administrative shell under which all Series operate. Substantive operations, assets, liabilities, and activities are conducted at the Series level unless otherwise specified in this Master Agreement. The Company maintains shared infrastructure, compliance obligations, and governance mechanisms that apply to all Series. The Company itself does not engage in substantive business operations except as necessary to administer the Series structure.

---

## ARTICLE II: SMART CONTRACT INTEGRATION

### Section 2.1 — Solana Program

The Company and its Series are managed in part through a smart contract program (the "Smart Contract" or "Program") deployed on the Solana blockchain at the following publicly available identifier, as required by the DAO Act:

**Program Address**: [SOLANA PROGRAM ADDRESS]

**Blockchain Network**: Solana Mainnet (Cluster: mainnet-beta)

This Program address constitutes the "publicly available identifier of any smart contract directly used to manage the DAO" as required by the DAO Act for inclusion in the certificate of formation and this Master Agreement. The same Program manages the Company and all of its Series.

### Section 2.2 — Master LLC Entity PDA

The Company is represented on the Solana blockchain by an Entity PDA (Program Derived Address) derived using the seeds `[b"entity", entity_id.as_bytes()]` at the Program address specified in Section 2.1. The Entity PDA stores the Company's metadata, authority pointer, token mint addresses, and the SHA-256 hash of this Master Agreement in the `charter_hash` field.

### Section 2.3 — Master LLC Security Token

The Company issues a master security token (the "Master Security Token") representing membership interests in the Company as a whole, deployed as an SPL Token-2022 token on the Solana blockchain at the following mint address:

**Master Security Token Mint Address**: [MASTER SECURITY TOKEN MINT ADDRESS]

The Master Security Token incorporates the following Token-2022 extensions:

(a) **Transfer Hook**: Enforces compliance checks on every transfer, including KYC verification when any single holder's balance would exceed the Informal Holder Threshold (as defined in Section 4.5), Restricted Person screening, and lock-up period enforcement;

(b) **Permanent Delegate**: Designates the Foundation's Squads multisig vault as the permanent delegate, enabling forced transfers exclusively for legal compliance purposes;

(c) **Metadata**: Points to legal documentation describing the rights, restrictions, and obligations associated with the Master Security Token;

(d) **Default Account State**: All token accounts are created in a frozen state and are unfrozen only upon satisfaction of applicable compliance requirements.

[IF FOR-PROFIT: The Master Security Token confers economic rights including the right to receive distributions from the Company's master-level assets and a pro rata share of liquidation proceeds from assets not allocated to any Series.]

[IF NON-PROFIT: The Master Security Token confers non-economic membership interests. The Company is prohibited from distributing profits to Members. Pursuant to the DAO Act, "all digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security."]

### Section 2.4 — Master LLC Utility Token

The Company issues a master utility token (the "Master Utility Token") conferring governance rights over the Company as a whole, deployed as an SPL Token-2022 token on the Solana blockchain at the following mint address:

**Master Utility Token Mint Address**: [MASTER UTILITY TOKEN MINT ADDRESS]

The Master Utility Token confers governance rights only, including the right to submit proposals, cast votes, and delegate voting power with respect to Company-level governance matters (including Series creation, Series dissolution, Foundation appointment, Master Agreement amendments, and shared infrastructure decisions). The Master Utility Token does not confer any economic rights. Pursuant to the 2023 Amendment to the DAO Act, "a governance token conferring no economic rights shall not be deemed a security."

The Master Utility Token incorporates the following Token-2022 extensions:

(a) **Transfer Hook**: Validates that the transferee is an Active Member or will become an Active Member upon receipt of the token;

(b) **Metadata**: Points to governance documentation describing the governance rights associated with the Master Utility Token.

### Section 2.5 — Smart Contract as Authoritative Membership Registry

Pursuant to the DAO Act, which provides that "membership in the DAO LLC can be based on the token holding criterion and tracked on-chain," the Smart Contract serves as the authoritative membership registry of the Company and each Series. The on-chain Member Registry maintained by the Smart Contract satisfies the Company's and each Series' obligation to maintain a register of members. No duplicate off-chain membership registry is required.

### Section 2.6 — Blockchain Records

Pursuant to the DAO Act, which provides that "written or paper records are not required if they are maintained on the blockchain," the Company and each Series may maintain any records required by the LLC Act or the DAO Act on the Solana blockchain in lieu of written or paper records.

### Section 2.7 — Smart Contract Technical Summary

The Smart Contract implements the following autonomous operations: entity management, series creation, share class creation, member registration, token issuance, utility-to-security token swaps with KYC gating, permissionless security-to-utility token swaps, governance proposal creation, token-weighted vote casting, proposal execution, KYC status management, and authority rotation. A detailed technical description is attached hereto as Exhibit A.

---

## ARTICLE III: FOUNDATION AS REPRESENTATIVE

### Section 3.1 — Designation as Special Delegate

[FOUNDATION NAME], a non-profit DAO LLC organized under the laws of the Republic of the Marshall Islands with registration number [FOUNDATION REGISTRATION NUMBER] (the "Foundation"), is hereby designated as a Special Delegate of the Company and all of its Series pursuant to the DAO Act. The Foundation is not a director, officer, trustee, supervisor, or manager of the Company or any Series. The Foundation is a representative with limited, enumerated authority as defined in this Article III.

### Section 3.2 — Scope of Authority

The Foundation's authority with respect to the Company and all Series is limited to the following administrative, compliance, and representative functions:

(a) Filing compliance documents with the Registrar of Corporations, including annual reports, Beneficial Owner Information Reports, and amendments to the Certificate of Formation, on behalf of the Company and all Series;

(b) Maintaining the registered agent relationship on behalf of the Company;

(c) Performing KYC verification for Members participating in Utility Token to Security Token swaps or crossing the Informal Holder Threshold, at both the master level and the Series level;

(d) Acting as escrow for all Utility Token to Security Token swap processes, including writing KYC verification hashes to the blockchain and approving swap transactions;

(e) Interfacing with banking institutions and financial service providers on behalf of the Company and its Series as authorized by governance vote;

(f) Executing governance-approved administrative actions that require off-chain execution or interaction with third parties;

(g) [IF FOR-PROFIT: Filing Gross Revenue Tax returns and remitting GRT payments on behalf of the Company and each for-profit Series;]

(h) Receiving service of process on behalf of the Company and all Series;

(i) Maintaining encrypted KYC records off-chain in compliance with applicable data protection requirements;

(j) Operating the Transfer Hook compliance infrastructure, including sanctions screening and Restricted Person monitoring, for the Company and all Series;

(k) Coordinating the creation and dissolution of Series, including filing necessary amendments with the Registrar.

### Section 3.3 — Limitations on Authority

The Foundation shall not, without prior approval by governance vote of the Master Utility Token Holders (for Company-level actions) or the relevant Series Utility Token Holders (for Series-level actions):

(a) Make business decisions on behalf of the Company or any Series;

(b) Issue, mint, burn, or transfer tokens outside the authorized mechanisms described in this Master Agreement and the applicable Series Operating Supplement;

(c) Change governance rules, voting thresholds, quorum requirements, or proposal parameters at any level;

(d) Access, transfer, or encumber treasury funds or assets of the Company or any Series;

(e) Enter into contracts, agreements, or obligations on behalf of the Company or any Series with a value exceeding [THRESHOLD AMOUNT];

(f) Amend this Master Agreement or any Series Operating Supplement;

(g) Create or dissolve any Series without prior governance approval;

(h) Expand the scope of its own authority beyond that enumerated in Section 3.2.

### Section 3.4 — Foundation Squads Multisig

The Foundation exercises its on-chain authority through a Squads Protocol v4 multisig wallet with the following configuration:

**Multisig Address**: [FOUNDATION SQUADS MULTISIG ADDRESS]

**Threshold**: [THRESHOLD]-of-[TOTAL] signers

**Time Lock**: [TIME LOCK DURATION] hours

The multisig controls three vaults:

(a) **Vault 0** (Foundation Operations): Funds allocated for the Foundation's operational expenses;

(b) **Vault 1** (Program Upgrade Authority): Authority to upgrade the Smart Contract, exercisable only upon governance approval;

(c) **Vault 2** (Entity Authority): Authority to sign administrative transactions on behalf of the Company and all Series.

### Section 3.5 — Custodial Arrangement

The Foundation holds one hundred percent (100%) of the Master Security Tokens and each Series' Security Tokens on behalf of the respective Security Token Holders as a custodial arrangement. The Foundation is the registered holder of all Security Tokens at both the master and Series levels. Beneficial ownership maps to individual token holders based on on-chain records maintained in MemberRecord PDAs and token balances in user PDA wallets.

### Section 3.6 — Removal of Foundation

The Foundation may be removed as Special Delegate by a governance vote of the Master Utility Token Holders requiring approval of not less than [SUPERMAJORITY THRESHOLD]% of the Master Utility Tokens cast, with a quorum of not less than [QUORUM]% of the total outstanding Master Utility Tokens. Upon removal, the Master Utility Token Holders shall designate a successor Special Delegate.

### Section 3.7 — No Fiduciary Duty

The Foundation, in its capacity as Special Delegate, does not owe fiduciary duties to the Company, any Series, or any Member beyond the express obligations set forth in this Master Agreement and the applicable Series Operating Supplements and the implied covenant of good faith and fair dealing.

---

## ARTICLE IV: MEMBERSHIP AT THE MASTER LEVEL

### Section 4.1 — Membership by Token Holding

Membership in the Company at the master level is determined by holding Master Security Tokens or Master Utility Tokens. Any person or entity holding one or more Master Security Tokens or Master Utility Tokens is a Member of the Company. A person may be a Member of the Company without being a member of any Series, and may be a member of one or more Series without holding Master-level tokens. Token holders automatically become Members without requiring separate onboarding. By acquiring tokens, a Member accepts and agrees to be bound by this Master Agreement.

### Section 4.2 — Membership in Series

Membership in a specific Series is determined by holding that Series' Security Tokens or Utility Tokens, as specified in the applicable Series Operating Supplement. A person may be a member of multiple Series simultaneously. Membership in a Series does not automatically confer membership at the master level, and membership at the master level does not automatically confer membership in any Series.

### Section 4.3 — Membership Transfer

Membership interests are transferred simultaneously with token transfers on-chain, as provided by the DAO Act. No separate transfer agreement, consent of existing Members, or registration is required, subject to the Transfer Hook's compliance checks.

### Section 4.4 — Automatic Dissociation

A Member is automatically dissociated from the Company at the master level when the Member's combined balance of Master Security Tokens and Master Utility Tokens reaches zero. A member is automatically dissociated from a Series when the member's combined balance of that Series' Security Tokens and Utility Tokens reaches zero.

### Section 4.5 — Informal Holder Threshold

Pursuant to the 2024 DAO Regulations, a Beneficial Owner is defined as a person who exercises control through "more than 25% of the LLC's interests or voting rights" (the "Informal Holder Threshold"). The following provisions apply at both the master level and the Series level:

(a) Members holding tokens representing twenty-five percent (25%) or less of the total outstanding supply of the applicable token class (at either the master or Series level) are not Beneficial Owners and may remain anonymous.

(b) Any Member whose holdings would exceed twenty-five percent (25%) of the total outstanding supply must complete full KYC verification before the transfer or issuance can be executed. The Transfer Hook enforces this at both levels.

(c) Members holding more than ten percent (10%) but not more than twenty-five percent (25%) of governance rights are classified as "Significant Holders" and must complete KYC with the local regulator.

(d) All founders and incorporators must complete full KYC at formation.

(e) At least one Ultimate Beneficial Owner ("UBO") must be identified. The Foundation satisfies this requirement.

(f) KYC for Members exceeding the Informal Holder Threshold is renewed annually in January.

### Section 4.6 — Restricted Persons

A "Restricted Person" is any individual or entity that is: (a) listed on sanctions lists maintained by FATF, the UN Security Council, HM Treasury, OFAC, or the EU; (b) a resident or national of a comprehensively sanctioned country; or (c) in violation of the Company's AML policies. Restricted Persons are automatically dissociated from the Company and all Series. The Transfer Hook rejects transfers involving Restricted Persons at both the master and Series levels.

### Section 4.7 — Privacy

Members holding twenty-five percent (25%) or less of any token class may remain anonymous at both the master and Series levels. The Company shall protect Member privacy to the maximum extent permitted by law.

---

## ARTICLE V: GOVERNANCE AT THE MASTER LEVEL

### Section 5.1 — Master-Level Governance

Governance of the Company at the master level is exercised by the Master Utility Token Holders through the Governance Program. Governance rights are proportional to Master Utility Token holdings (one vote per Master Utility Token).

### Section 5.2 — Master-Level Governance Scope

Master-level governance has authority over the following matters:

(a) Creation of new Series;

(b) Dissolution of existing Series;

(c) Appointment and removal of the Foundation as Special Delegate;

(d) Amendments to this Master Agreement;

(e) Upgrades to the Smart Contract;

(f) Management of assets held at the master level (not allocated to any Series);

(g) Shared infrastructure, compliance, and administrative decisions affecting all Series;

(h) [IF FOR-PROFIT: Distributions from master-level assets to Master Security Token Holders;]

(i) Dissolution of the Company.

### Section 5.3 — Master-Level Proposal Types and Approval Thresholds

| Proposal Type | Approval Threshold | Quorum |
|---|---|---|
| Ordinary Resolutions (shared infrastructure, administrative) | [ORDINARY THRESHOLD]% of votes cast | [ORDINARY QUORUM]% of outstanding Master Utility Tokens |
| Special Resolutions (Master Agreement amendments, Series creation/dissolution) | [SPECIAL THRESHOLD]% of votes cast | [SPECIAL QUORUM]% of outstanding Master Utility Tokens |
| Extraordinary Resolutions (Company dissolution, Foundation removal) | [EXTRAORDINARY THRESHOLD]% of votes cast | [EXTRAORDINARY QUORUM]% of outstanding Master Utility Tokens |
| Smart Contract Upgrades | [UPGRADE THRESHOLD]% of votes cast | [UPGRADE QUORUM]% of outstanding Master Utility Tokens |

### Section 5.4 — Master-Level Proposal Process

(a) Any Master Utility Token Holder holding at least [MINIMUM PROPOSAL TOKENS] Master Utility Tokens may submit a proposal.

(b) Each proposal has a voting period of [VOTING PERIOD] days.

(c) Members cast votes weighted by Master Utility Token holdings.

(d) Approved proposals may be executed on-chain or, where off-chain execution is required, by the Foundation within [EXECUTION PERIOD] days.

### Section 5.5 — Voting Delegation

Master Utility Token Holders may delegate voting power to another Member. Delegation is revocable at any time and does not transfer token ownership.

### Section 5.6 — Master Agreement Amendments

Amendments to this Master Agreement require approval as a Special Resolution at the master level. The SHA-256 hash of the amended Master Agreement shall be written to the Entity PDA's `charter_hash` field. The amended Master Agreement shall be filed with the Registrar.

---

## PART TWO: SERIES FRAMEWORK

---

## ARTICLE VI: SERIES CREATION AND STRUCTURE

### Section 6.1 — Series Creation

The Company may create one or more series (each, a "Series") through the master-level Governance Program. Creation of a Series requires approval as a Special Resolution of the Master Utility Token Holders. Each Series is registered under the Company's Certificate of Formation and documented in a Series Operating Supplement that is appended to this Master Agreement and filed with the Registrar.

### Section 6.2 — Series PDA

Each Series is represented on the Solana blockchain by a Series PDA derived using seeds `[b"series", entity_pda.key().as_ref(), series_name.as_bytes()]`. The Series PDA stores the Series' metadata, parent entity reference, token mint addresses, member count, and charter hash.

### Section 6.3 — Series Token Structure

Each Series issues its own pair of tokens:

(a) **Series Security Token**: An SPL Token-2022 token representing [IF FOR-PROFIT: economic ownership interests in the Series] [IF NON-PROFIT: non-economic membership interests in the Series], with Transfer Hook, Permanent Delegate, Metadata, and Default Account State extensions;

(b) **Series Utility Token**: An SPL Token-2022 token conferring governance rights within the Series, with Transfer Hook and Metadata extensions.

The mint addresses for each Series' tokens are specified in the applicable Series Operating Supplement.

### Section 6.4 — Series Independence

Each Series established under this Master Agreement shall have:

(a) **Separate Assets**: Its own treasury, tokens, property, and contractual rights, held separately from all other Series and from the master-level assets of the Company;

(b) **Separate Liabilities**: Its own debts, obligations, and liabilities. Creditors of one Series shall not have recourse to the assets of any other Series or to the master-level assets of the Company not allocated to such Series. The liabilities incurred with respect to a particular Series shall be enforceable against the assets of such Series only;

(c) **Separate Governance**: Its own voting rules, quorum requirements, proposal types, and management model, as specified in its Series Operating Supplement;

(d) **Separate Membership**: Its own member composition, which may differ from the membership of the Company at the master level and from other Series.

### Section 6.5 — Inter-Series Liability Shield

The debts, liabilities, obligations, and expenses incurred, contracted for, or otherwise existing with respect to a particular Series shall be enforceable against the assets of such Series only, and not against the assets of the Company generally, the master-level assets, or any other Series. No creditor of any Series shall have any right to satisfy a claim against such Series from the assets of any other Series or from the master-level assets of the Company. This liability shield is consistent with the Series LLC provisions of the 2023 Amendments to the DAO Act and follows the Delaware Series LLC precedent codified in 6 Del. C. Section 18-215.

### Section 6.6 — Master LLC Liability

The master-level assets of the Company (assets not allocated to any Series) are subject only to the master-level liabilities of the Company (liabilities not attributable to any Series). The master LLC does not guarantee, indemnify, or assume the liabilities of any Series.

### Section 6.7 — Series Operating Supplements

Each Series shall be governed by a Series Operating Supplement appended to this Master Agreement. Each Series Operating Supplement shall specify, at a minimum:

(a) Series name and purpose;

(b) Series Security Token mint address and specifications;

(c) Series Utility Token mint address and specifications;

(d) Series governance parameters (proposal types, approval thresholds, quorum requirements, voting period);

(e) Series management mode (member-managed or algorithmically managed);

(f) Series-specific membership criteria, if any, beyond token holding;

(g) [IF FOR-PROFIT: Series distribution policies and mechanisms;]

(h) Initial members and token allocations for the Series;

(i) Any provisions that differ from or supplement this Master Agreement.

To the extent a Series Operating Supplement is silent on any matter, the terms of this Master Agreement shall govern.

### Section 6.8 — Series Dissolution

A Series may be dissolved by:

(a) A Special Resolution of the Master Utility Token Holders; or

(b) An Extraordinary Resolution of the Series' Utility Token Holders (if the Series has its own governance mechanism), subject to satisfaction of all liabilities of the Series.

Dissolution of a Series does not cause dissolution of the Company or any other Series. Upon dissolution of a Series, the Foundation shall wind up the affairs of the Series, satisfy all liabilities, and [IF FOR-PROFIT: distribute remaining assets to the Series' Security Token Holders in proportion to their holdings] [IF NON-PROFIT: distribute remaining assets in a manner consistent with the non-profit purpose of the Company and the Series].

---

## ARTICLE VII: SERIES-LEVEL GOVERNANCE

### Section 7.1 — Series Governance Autonomy

Each Series may establish its own governance mechanism through the Smart Contract, as specified in its Series Operating Supplement. Series-level governance decisions do not require master-level approval unless the decision affects the Company or other Series.

### Section 7.2 — Series Governance Scope

Series-level governance has authority over:

(a) Operations and activities within the Series;

(b) Management of Series-level assets and treasury;

(c) [IF FOR-PROFIT: Distributions from Series-level assets to Series Security Token Holders;]

(d) Series-level membership criteria beyond token holding;

(e) Series Operating Supplement amendments (subject to consistency with this Master Agreement);

(f) Any other matter specified in the Series Operating Supplement.

### Section 7.3 — Conflict Between Master and Series Governance

In the event of a conflict between a master-level governance decision and a Series-level governance decision, the master-level decision shall prevail to the extent of the conflict, provided that master-level governance may not unilaterally access, transfer, or encumber the assets of a Series except upon dissolution of the Series.

### Section 7.4 — Series Governance Parameters

Each Series Operating Supplement shall specify the governance parameters for the Series, including proposal types, approval thresholds, quorum requirements, voting period, minimum tokens for proposal submission, and delegation rules. If a Series Operating Supplement does not specify governance parameters, the master-level governance parameters in Article V shall apply to the Series.

---

## ARTICLE VIII: [NON-PROFIT RESTRICTIONS / DISTRIBUTIONS]

[IF NON-PROFIT, USE THE FOLLOWING:]

### Section 8.1 — Prohibition on Profit Distribution

No part of the income or profit of the Company or any Series shall be distributable to Members, directors, officers, or any other person. Neither the Company nor any Series shall declare or pay dividends, profit shares, or any other form of profit distribution.

### Section 8.2 — Permitted Treasury Activities

Notwithstanding Section 8.1, the Company and each Series may: (a) hold, manage, and invest treasury assets; (b) make grants and disbursements in furtherance of the non-profit purpose, as approved through the applicable Governance Program; (c) pay reasonable compensation for services rendered; (d) reimburse expenses incurred by Members, Special Delegates, or service providers; (e) fund the operations of Series.

### Section 8.3 — Prohibited Activities

The Company and each Series shall not: (a) distribute profits to Members; (b) engage in propaganda or legislative influence (with limited exceptions for nonpartisan analysis); (c) participate in political campaigns; (d) engage in activities inconsistent with the non-profit purpose.

### Section 8.4 — Securities Treatment

All digital assets issued by the Company and its Series are not digital securities under Marshall Islands law. Pursuant to the DAO Act, "all digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security." This is an absolute statutory safe harbor.

[IF FOR-PROFIT, USE THE FOLLOWING:]

### Section 8.1 — Distributions to Security Token Holders

The Company and each Series may distribute profits, revenues, or other economic benefits to Security Token Holders in proportion to their holdings. Distributions from master-level assets require approval by the Master Utility Token Holders. Distributions from Series-level assets require approval by the applicable Series Utility Token Holders.

### Section 8.2 — Distribution Mechanics

Distributions shall be executed through the Smart Contract by transferring the distribution amount from the applicable treasury to token accounts in proportion to Security Token holdings at the snapshot time specified in the approved proposal.

### Section 8.3 — No Distribution via Utility Tokens

No distributions shall be made based on Utility Token holdings at any level.

### Section 8.4 — Tax Obligations

The Company is subject to the Gross Revenue Tax ("GRT") of three percent (3%) on earned income and interest. Capital gains and dividends are excluded from GRT. No pass-through taxation to Members. The Foundation files GRT returns and remits payments on behalf of the Company and each Series.

### Section 8.5 — No Withholding Tax

Distributions are not subject to withholding tax under Marshall Islands law.

### Section 8.6 — Securities Treatment

The Security Tokens issued by the Company and its Series are not automatically classified as securities under Marshall Islands law. The DAO Act exempts the Company from the Marshall Islands Securities and Investment Act "to the extent that a DAO LLC is not issuing, selling, exchanging or transferring any digital securities to residents of the Republic." Governance tokens conferring no economic rights are not securities pursuant to the 2023 Amendment.

---

## ARTICLE IX: LIABILITY AND FIDUCIARY DUTIES

### Section 9.1 — Limited Liability

No Member shall be liable as such for the liabilities of the Company or any Series, regardless of any failure to observe formalities. The debts, obligations, and liabilities of the Company or any Series shall be solely the debts, obligations, and liabilities of the Company or such Series. A Member's liability is limited to the value of the Member's capital contribution, if any.

### Section 9.2 — Waiver of Fiduciary Duties

To the maximum extent permitted by the DAO Act and the LLC Act, this Master Agreement does not create or impose any fiduciary duty on any Member, Special Delegate, or other person. Each Member waives, to the fullest extent permitted by law, all fiduciary duties that may be implied by law or equity. Duties are limited to those expressly set forth herein and the implied covenant of good faith and fair dealing.

### Section 9.3 — Implied Covenant of Good Faith and Fair Dealing

Each Member and the Foundation are subject to the implied covenant of good faith and fair dealing, which cannot be waived under the DAO Act.

### Section 9.4 — Open-Source Software Immunity

Pursuant to the 2023 Amendments, the Company, its Series, and their Members shall not be liable for any open-source software created, published, or contributed to by the Company or any Series, even if third parties misuse such software.

### Section 9.5 — Indemnification

The Company shall indemnify and hold harmless each Member, the Foundation, and any Special Delegate from and against any claims, losses, damages, liabilities, costs, and expenses arising out of participation in the Company or any Series, except for willful misconduct, gross negligence, or breach of this Master Agreement.

### Section 9.6 — Corporate Veil

The Company and each Series maintain separate corporate veils. The corporate veil of the Company or any Series shall not be pierced except upon a finding of commingling of funds, inadequate capitalization, fraud, or alter ego status, in accordance with the LLC Act and Delaware precedent.

---

## ARTICLE X: COMPLIANCE

### Section 10.1 — Annual Report

The Foundation shall file a consolidated annual report with the Registrar between January 1st and March 31st covering the Company and all Series. The annual report shall contain beneficial ownership information, leadership and management details, Series status, and any structural changes.

### Section 10.2 — Beneficial Owner Information Report

The Company shall maintain and file a BOIR identifying each Beneficial Owner at both the master and Series levels. A Beneficial Owner is a person who exercises control through more than twenty-five percent (25%) of interests or voting rights at any level.

### Section 10.3 — KYC Obligations

KYC obligations apply at both the master and Series levels as set forth in Section 4.5. The Foundation monitors thresholds at both levels through the Transfer Hook infrastructure.

### Section 10.4 — AML Policy

The Company shall adopt a single AML policy applicable to the Company and all Series. The Foundation implements and maintains the AML policy.

### Section 10.5 — Smart Contract Updates

Material updates to the Smart Contract shall be disclosed to the Registrar and require approval as a Smart Contract Upgrade proposal at the master level.

### Section 10.6 — Corporate Continuation

The Company shall complete corporate continuation annually for the Company and all Series. Failure to comply may result in penalties of USD 500 per day, fines of up to USD 10,000, and potential cancellation of the Certificate of Formation.

### Section 10.7 — Series-Level Compliance

Each Series shall comply with the compliance requirements of this Master Agreement. The Foundation coordinates compliance for all Series and files consolidated reports where permitted by the Registrar.

---

## ARTICLE XI: SWAP MECHANISM

### Section 11.1 — Master-Level Swaps

Members may swap between Master Security Tokens and Master Utility Tokens through the Smart Contract, subject to the following:

**(a) Utility to Security Swap (KYC Required)**: The Member completes KYC through the Foundation. The Foundation writes the KYC hash to the MemberRecord PDA. The Foundation approves the swap through its multisig. The Smart Contract burns Utility Tokens and mints Security Tokens.

**(b) Security to Utility Swap (No KYC Required)**: The Smart Contract executes the swap directly, burning Security Tokens and minting Utility Tokens. No Foundation approval required.

### Section 11.2 — Series-Level Swaps

Members may swap between a Series' Security Tokens and Utility Tokens under the same mechanism and conditions as master-level swaps. KYC verification performed for master-level swaps satisfies the KYC requirement for Series-level swaps, and vice versa, provided the KYC has not expired.

### Section 11.3 — Cross-Level Swaps

Master-level tokens may not be swapped for Series-level tokens, and Series-level tokens may not be swapped for Master-level tokens, through the swap mechanism. Such exchanges, if desired, must be conducted through governance-approved mechanisms.

---

## ARTICLE XII: DISSOLUTION

### Section 12.1 — Company Dissolution Events

The Company shall be dissolved upon:

(a) An Extraordinary Resolution of the Master Utility Token Holders;

(b) A decree of judicial dissolution;

(c) Cancellation of the Certificate of Formation for non-compliance;

(d) Any event making it unlawful for the Company to continue.

### Section 12.2 — Effect of Company Dissolution on Series

Dissolution of the Company causes the dissolution of all Series. Each Series shall be wound up separately, with its assets applied to its own liabilities before any remaining assets are distributed.

### Section 12.3 — Winding Up

Upon dissolution, the Foundation shall wind up the affairs of each Series separately and the Company, which shall include:

(a) For each Series: collecting all Series assets, satisfying all Series liabilities, and distributing remaining Series assets [IF FOR-PROFIT: to Series Security Token Holders in proportion to holdings] [IF NON-PROFIT: to non-profit organizations as determined by governance vote];

(b) For the Company: collecting all master-level assets, satisfying all master-level liabilities, and distributing remaining master-level assets [IF FOR-PROFIT: to Master Security Token Holders in proportion to holdings] [IF NON-PROFIT: to non-profit organizations as determined by governance vote];

(c) Filing a notice of dissolution with the Registrar.

### Section 12.4 — Series Dissolution Without Company Dissolution

Dissolution of one or more Series does not cause dissolution of the Company or any other Series.

---

## ARTICLE XIII: DISPUTE RESOLUTION

### Section 13.1 — Mandatory Negotiation

Any dispute arising out of or relating to this Master Agreement or any Series Operating Supplement shall first be submitted to mandatory negotiation in good faith for thirty (30) days.

### Section 13.2 — Arbitration

Unresolved disputes shall be settled by binding arbitration administered by the International Centre for Dispute Resolution ("ICDR") in accordance with its International Arbitration Rules. The seat of arbitration shall be the Republic of the Marshall Islands. The language shall be English. The tribunal shall consist of [ONE / THREE] arbitrator(s).

### Section 13.3 — Governing Law

This Master Agreement, all Series Operating Supplements, and all disputes shall be governed by Marshall Islands law.

### Section 13.4 — No Court Jurisdiction

No court shall have jurisdiction over claims arising from this Master Agreement or any Series Operating Supplement, except for enforcement of arbitral awards and mandatory provisions of the DAO Act.

### Section 13.5 — Costs

Each party bears its own costs unless the arbitral tribunal orders otherwise.

---

## ARTICLE XIV: GENERAL PROVISIONS

### Section 14.1 — Entire Agreement

This Master Agreement, together with the Certificate of Formation, all Series Operating Supplements, and the Exhibits hereto, constitutes the entire agreement among the Members.

### Section 14.2 — Severability

Invalid provisions shall be modified to the minimum extent necessary to be enforceable while preserving original intent.

### Section 14.3 — Electronic Execution

This Master Agreement and all Series Operating Supplements may be executed electronically, including by wallet signature. Acquisition of tokens constitutes acceptance of this Master Agreement and any applicable Series Operating Supplement.

### Section 14.4 — Notices

Notices may be delivered through the governance mechanism, on-chain messaging, or other designated means.

### Section 14.5 — No Waiver

Failure to enforce any provision shall not constitute a waiver.

### Section 14.6 — Counterparts

This Master Agreement may be executed in counterparts.

---

## PART THREE: EXHIBITS

---

### Exhibit A: Smart Contract Technical Summary

**Program Address**: [SOLANA PROGRAM ADDRESS]

**Blockchain**: Solana Mainnet (mainnet-beta)

**Framework**: Anchor

**Token Standard**: SPL Token-2022

**Instructions**:

| Instruction | Description |
|---|---|
| `initialize_entity()` | Creates the Entity PDA for the master LLC |
| `create_series()` | Creates a Series PDA within the entity |
| `create_share_class()` | Deploys Token-2022 mints at master or Series level |
| `add_member()` | Creates MemberRecord PDAs at master or Series level |
| `issue_shares()` | Mints tokens to member accounts |
| `swap_utility_to_security()` | KYC-gated swap at master or Series level |
| `swap_security_to_utility()` | Permissionless swap at master or Series level |
| `create_proposal()` | Creates governance proposals at master or Series level |
| `cast_vote()` | Records token-weighted votes |
| `execute_proposal()` | Executes approved proposals |
| `update_member_kyc()` | Writes KYC hash to MemberRecord |
| `rotate_authority()` | Changes entity or Series authority |

**PDA Seed Design**:

| PDA | Seeds |
|---|---|
| Entity | `[b"entity", entity_id.as_bytes()]` |
| Series | `[b"series", entity_pda.key().as_ref(), series_name.as_bytes()]` |
| Share Class | `[b"share_class", entity_pda.key().as_ref(), class_name.as_bytes()]` |
| Member Record | `[b"member", entity_pda.key().as_ref(), member_wallet.key().as_ref()]` |
| Governance | `[b"governance", entity_pda.key().as_ref()]` |
| Proposal | `[b"proposal", entity_pda.key().as_ref(), &proposal_id.to_le_bytes()]` |
| Vote Record | `[b"vote", proposal_pda.key().as_ref(), member_wallet.key().as_ref()]` |

**Transfer Hook Logic**: Every Token-2022 transfer at both master and Series level invokes the Transfer Hook Program, which validates: (1) both parties are Active Members at the applicable level; (2) neither party is a Restricted Person; (3) for Security Tokens, the receiver's post-transfer balance does not exceed the Informal Holder Threshold without KYC; (4) KYC has not expired; (5) lock-up periods are respected.

### Exhibit B: Master-Level Token Specifications

**Master Security Token**:

| Parameter | Value |
|---|---|
| Mint Address | [MASTER SECURITY TOKEN MINT ADDRESS] |
| Token Standard | SPL Token-2022 |
| Decimals | [DECIMALS] |
| Initial Supply | [INITIAL SUPPLY] |
| Extensions | Transfer Hook, Permanent Delegate, Metadata, Default Account State (Frozen) |
| Permanent Delegate | [FOUNDATION SQUADS VAULT 2 ADDRESS] |
| Transfer Hook Program | [TRANSFER HOOK PROGRAM ADDRESS] |

**Master Utility Token**:

| Parameter | Value |
|---|---|
| Mint Address | [MASTER UTILITY TOKEN MINT ADDRESS] |
| Token Standard | SPL Token-2022 |
| Decimals | [DECIMALS] |
| Initial Supply | [INITIAL SUPPLY] |
| Extensions | Transfer Hook, Metadata |
| Transfer Hook Program | [TRANSFER HOOK PROGRAM ADDRESS] |

### Exhibit C: Foundation Details

| Detail | Value |
|---|---|
| Foundation Name | [FOUNDATION NAME] |
| Registration Number | [FOUNDATION REGISTRATION NUMBER] |
| Jurisdiction | Republic of the Marshall Islands |
| Entity Type | Non-Profit DAO LLC |
| Squads Multisig Address | [FOUNDATION SQUADS MULTISIG ADDRESS] |
| Multisig Threshold | [THRESHOLD]-of-[TOTAL] |
| Time Lock | [TIME LOCK DURATION] hours |
| Vault 0 (Operations) | [VAULT 0 ADDRESS] |
| Vault 1 (Upgrade Authority) | [VAULT 1 ADDRESS] |
| Vault 2 (Entity Authority) | [VAULT 2 ADDRESS] |

### Exhibit D: Master-Level Governance Parameters

| Parameter | Value |
|---|---|
| Ordinary Resolution Threshold | [ORDINARY THRESHOLD]% |
| Ordinary Resolution Quorum | [ORDINARY QUORUM]% |
| Special Resolution Threshold | [SPECIAL THRESHOLD]% |
| Special Resolution Quorum | [SPECIAL QUORUM]% |
| Extraordinary Resolution Threshold | [EXTRAORDINARY THRESHOLD]% |
| Extraordinary Resolution Quorum | [EXTRAORDINARY QUORUM]% |
| Upgrade Threshold | [UPGRADE THRESHOLD]% |
| Upgrade Quorum | [UPGRADE QUORUM]% |
| Minimum Tokens for Proposal | [MINIMUM PROPOSAL TOKENS] |
| Voting Period | [VOTING PERIOD] days |

### Exhibit E: Initial Series

| Series Name | Purpose | Security Token Mint | Utility Token Mint | Series Operating Supplement |
|---|---|---|---|---|
| [SERIES 1 NAME] | [PURPOSE] | [MINT ADDRESS] | [MINT ADDRESS] | Supplement 1 |
| [SERIES 2 NAME] | [PURPOSE] | [MINT ADDRESS] | [MINT ADDRESS] | Supplement 2 |

### Exhibit F: Initial Members (Master Level)

| Member | Wallet Address | Token Type | Amount | KYC Status |
|---|---|---|---|---|
| [FOUNDATION NAME] | [FOUNDATION WALLET] | Master Security Token | [AMOUNT] | Verified (UBO) |
| [MEMBER NAME / ANONYMOUS] | [WALLET ADDRESS] | [TOKEN TYPE] | [AMOUNT] | [STATUS] |

---

## PART FOUR: SERIES OPERATING SUPPLEMENT TEMPLATE

---

### SERIES OPERATING SUPPLEMENT NO. [NUMBER]

### [SERIES NAME]

**Effective Date**: [DATE]

**Series of**: [ENTITY NAME] DAO LLC

This Series Operating Supplement No. [NUMBER] (this "Supplement") is adopted pursuant to the Master Operating Agreement of [ENTITY NAME] DAO LLC dated [MASTER AGREEMENT DATE] (the "Master Agreement"). This Supplement is incorporated into and subject to the Master Agreement. Capitalized terms used but not defined in this Supplement have the meanings given in the Master Agreement.

**Section 1 — Series Name**: [SERIES NAME]

**Section 2 — Series Purpose**: [DESCRIBE SERIES PURPOSE]

**Section 3 — Management Mode**: [MEMBER-MANAGED / ALGORITHMICALLY MANAGED]

**Section 4 — Series Token Addresses**:

| Token | Mint Address | Decimals | Initial Supply |
|---|---|---|---|
| Series Security Token | [SERIES SECURITY TOKEN MINT ADDRESS] | [DECIMALS] | [INITIAL SUPPLY] |
| Series Utility Token | [SERIES UTILITY TOKEN MINT ADDRESS] | [DECIMALS] | [INITIAL SUPPLY] |

**Section 5 — Series Governance Parameters**:

| Parameter | Value |
|---|---|
| Ordinary Resolution Threshold | [THRESHOLD]% |
| Ordinary Resolution Quorum | [QUORUM]% |
| Special Resolution Threshold | [THRESHOLD]% |
| Special Resolution Quorum | [QUORUM]% |
| Extraordinary Resolution Threshold | [THRESHOLD]% |
| Extraordinary Resolution Quorum | [QUORUM]% |
| Minimum Tokens for Proposal | [AMOUNT] |
| Voting Period | [DAYS] days |

**Section 6 — Series-Specific Provisions**: [ANY PROVISIONS THAT DIFFER FROM OR SUPPLEMENT THE MASTER AGREEMENT]

**Section 7 — Initial Series Members**:

| Member | Wallet Address | Token Type | Amount | KYC Status |
|---|---|---|---|---|
| [MEMBER] | [WALLET] | [TYPE] | [AMOUNT] | [STATUS] |

[IF FOR-PROFIT:]

**Section 8 — Distribution Policy**: [SERIES-SPECIFIC DISTRIBUTION RULES, IF DIFFERENT FROM MASTER AGREEMENT]

---

**IN WITNESS WHEREOF**, the Members have executed this Master Operating Agreement as of the Effective Date by acquiring Master Security Tokens, Master Utility Tokens, or Series tokens of the Company, which acquisition constitutes acceptance of and agreement to this Master Operating Agreement and any applicable Series Operating Supplement.

[ENTITY NAME] DAO LLC
Republic of the Marshall Islands
Registration Number: [REGISTRATION NUMBER]
Effective Date: [DATE]
