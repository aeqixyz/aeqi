# OPERATING AGREEMENT OF [ENTITY_NAME] DAO LLC

## A For-Profit Decentralized Autonomous Organization Limited Liability Company

## Organized Under the Laws of the Republic of the Marshall Islands

---

> **TEMPLATE PREAMBLE**
>
> **What this document covers:** This is a template Operating Agreement for a for-profit DAO LLC organized under the laws of the Republic of the Marshall Islands. It is designed for decentralized autonomous organizations that operate on the Solana blockchain, use SPL Token (Token-2022) governance tokens, and manage treasury operations through Squads Protocol multisig. The template follows the structure established by the Odos DAO LLC operating agreement (the most detailed publicly available Marshall Islands DAO LLC agreement) and the Pyth DAO LLC (which established Solana-specific precedent).
>
> **How to use this template:**
> 1. Replace all placeholders (marked with `[BRACKETS]`) with your entity's specific information.
> 2. Where you see `[CHOOSE: option A / option B]`, select the appropriate option and delete the alternative.
> 3. Review all approval thresholds, quorum requirements, and voting periods -- these should be customized to your DAO's size and governance needs.
> 4. Complete all Exhibits (A through E) with your specific member, signer, delegate, and smart contract information.
> 5. Engage a qualified attorney licensed in the Republic of the Marshall Islands to review and finalize this agreement before filing.
> 6. After finalization, compute the SHA-256 hash of the agreement and anchor it on-chain in the Entity PDA.
>
> **Legal hierarchy:** Under Marshall Islands law, the hierarchy of authority is: (1) DAO Act and RMI law, (2) LLC Act, (3) this Operating Agreement, (4) Smart Contract Code. This means the written agreement always overrides smart contract behavior in case of conflict.
>
> **Applicable law:** This template references the Decentralized Autonomous Organization Act of 2022, the DAO Act Amendments of 2023, the DAO Regulations of 2024, and the Limited Liability Company Act (Title 52, Chapter 4, MIRC). All DAO LLCs are also subject to Delaware precedent where no conflicting RMI precedent exists.
>
> **Solana-specific provisions:** This template is written for DAOs operating on Solana. It references SPL Token-2022 (with Transfer Hook, Permanent Delegate, and Metadata extensions), Squads Protocol v4 multisig, and Anchor-based governance programs. If your DAO operates on a different blockchain, this template will require substantial modification.
>
> **THIS TEMPLATE DOES NOT CONSTITUTE LEGAL ADVICE.** See full disclaimers at the end of this document.

---

**Effective Date**: [EFFECTIVE_DATE]

**Entity Registration Number**: [REGISTRATION_NUMBER]

**Registered Agent**: [REGISTERED_AGENT_NAME], Marshall Islands

---

## RECITALS

WHEREAS, [ENTITY_NAME] DAO LLC (the "Company") has been organized as a for-profit Decentralized Autonomous Organization Limited Liability Company under the laws of the Republic of the Marshall Islands pursuant to the Decentralized Autonomous Organization Act of 2022, as amended by the DAO Act Amendments of 2023 (collectively, the "DAO Act"), the DAO Regulations of 2024 (the "DAO Regulations"), and the Limited Liability Company Act (Title 52, Chapter 4 of the Marshall Islands Revised Code) (the "LLC Act");

WHEREAS, the Company's Certificate of Formation was filed with the Registrar of Corporations of the Republic of the Marshall Islands on [FORMATION_DATE] and issued on [CERTIFICATE_DATE];

WHEREAS, the Members desire to enter into this Operating Agreement to set forth their respective rights, duties, obligations, and liabilities, and to establish the governance, management, and operational framework of the Company;

WHEREAS, the Members intend that the Company shall be governed in part through on-chain smart contracts deployed on the Solana blockchain, and that the on-chain Member Registry maintained through such smart contracts shall serve as the authoritative record of membership in the Company;

NOW, THEREFORE, in consideration of the mutual covenants and agreements hereinafter set forth and for other good and valuable consideration, the receipt and sufficiency of which are hereby acknowledged, the Members agree as follows:

---

## ARTICLE I: ORGANIZATION

### Section 1.1 -- Name

The name of the Company is **[ENTITY_NAME] DAO LLC** (the "Company"). The Company may conduct business under such name or any other name approved by the Members through the Governance Program.

### Section 1.2 -- Entity Type

The Company is organized as a for-profit Decentralized Autonomous Organization Limited Liability Company under the laws of the Republic of the Marshall Islands.

### Section 1.3 -- Governing Law

This Agreement and the rights, duties, and obligations of the Members shall be governed by and construed in accordance with the laws of the Republic of the Marshall Islands, including without limitation:

(a) The Decentralized Autonomous Organization Act of 2022;

(b) The DAO Act Amendments of 2023;

(c) The DAO Regulations of 2024;

(d) The Limited Liability Company Act (Title 52, Chapter 4, MIRC);

(e) Applicable Delaware precedent, to the extent that Marshall Islands law follows Delaware precedent and no conflicting RMI precedent or statute exists.

### Section 1.4 -- Formation Date

The Company was formed on [FORMATION_DATE] upon the filing of the Certificate of Formation with the Registrar of Corporations of the Republic of the Marshall Islands.

### Section 1.5 -- Registered Agent and Office

(a) The Company's registered agent in the Republic of the Marshall Islands is [REGISTERED_AGENT_NAME], with its registered office at [REGISTERED_AGENT_ADDRESS].

(b) The Company shall maintain a registered agent and registered office in the Marshall Islands continuously, as required by applicable law.

(c) The registered agent may be changed by a resolution approved through the Governance Program.

### Section 1.6 -- Purpose

The purpose of the Company is to [PURPOSE_STATEMENT].

Without limiting the generality of the foregoing, the Company may engage in any lawful activity permitted under the DAO Act and the LLC Act, including but not limited to:

(a) [SPECIFIC_ACTIVITY_1];

(b) [SPECIFIC_ACTIVITY_2];

(c) [SPECIFIC_ACTIVITY_3];

(d) Holding, managing, and disposing of digital assets, tokens, and other property, including but not limited to SOL, USDC, and SPL tokens on the Solana blockchain;

(e) Entering into contracts, agreements, and arrangements with third parties;

(f) Distributing profits and revenues to Members in accordance with this Agreement;

(g) Any and all activities incidental to, or necessary for, the accomplishment of the foregoing purposes.

### Section 1.7 -- Term

The Company shall have perpetual existence unless dissolved in accordance with Article IX of this Agreement or as otherwise required by applicable law.

### Section 1.8 -- Legal Hierarchy

In the event of any conflict between the sources of authority governing the Company, the following hierarchy shall apply, in descending order of priority:

(a) The DAO Act, DAO Regulations, and applicable laws of the Republic of the Marshall Islands;

(b) The LLC Act (Title 52, Chapter 4, MIRC);

(c) This Operating Agreement (including any written amendments hereto);

(d) Smart Contract Code deployed on the Solana blockchain.

For the avoidance of doubt, where the Smart Contract Code produces a result inconsistent with this Operating Agreement, this Operating Agreement shall control. This hierarchy ensures legal certainty while enabling on-chain governance.

### Section 1.9 -- Smart Contract Identification

(a) The Company is governed in part by Smart Contract Code deployed on the **Solana** blockchain (Mainnet-Beta, cluster URL: https://api.mainnet-beta.solana.com) at the following address(es):

| Smart Contract | Address | Type | Purpose |
|---------------|---------|------|---------|
| Governance Program | [PROGRAM_ADDRESS] | Anchor Program | On-chain governance, proposals, and voting |
| Governance Token Mint | [TOKEN_MINT] | Token-2022 SPL Token | Membership Interest representation |
| Transfer Hook Program | [TRANSFER_HOOK_ADDRESS] | Anchor Program | Transfer restriction enforcement (KYC, sanctions, lock-up) |
| Cap Table Program | [CAP_TABLE_PROGRAM_ADDRESS] | Anchor Program | Entity, share class, and member record management |
| Multisig Vault | [MULTISIG_ADDRESS] | Squads Protocol v4 | Treasury management and administrative operations |
| [ADDITIONAL_CONTRACT_NAME] | [ADDITIONAL_CONTRACT_ADDRESS] | [ADDITIONAL_CONTRACT_TYPE] | [ADDITIONAL_CONTRACT_PURPOSE] |

(b) The Company utilizes the **Squads Protocol** multisig (version 4) as the primary governance mechanism for treasury management and administrative operations requiring multi-party approval. The Squads Vault PDA serves as the authority on the Entity account, requiring multisig approval for all critical operations including share issuance, smart contract upgrades, and treasury disbursements.

(c) Membership Interests in the Company are represented by **Token-2022 SPL tokens** (the "Governance Tokens") minted under the Token-2022 program (Token Program ID: `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`) at mint address [TOKEN_MINT] on the Solana blockchain.

(d) The following Token-2022 extensions are enabled on the Governance Token mint:

   (i) **Transfer Hook** -- Enforces compliance checks (KYC verification, sanctions screening, lock-up periods) on every token transfer via the program at [TRANSFER_HOOK_ADDRESS];

   (ii) **Permanent Delegate** -- Enables the Company to effect forced transfers or burns for legal compliance (court orders, sanctions enforcement), with the delegate set to the Squads Vault PDA at [MULTISIG_ADDRESS];

   (iii) **Metadata** -- Stores on-chain share class details, legal document URI, and entity information;

   (iv) **[CHOOSE: Default Account State (Frozen) -- New token accounts start frozen until KYC is verified / No Default Account State extension]**;

   (v) [ADDITIONAL_EXTENSIONS].

(e) The foregoing smart contract addresses constitute the "publicly available identifier of any smart contract directly used to manage the DAO" as required by the DAO Act.

(f) The Entity PDA stores a SHA-256 hash of this Operating Agreement (the "Charter Hash"), linking the on-chain entity to this legal document. The Charter Hash as of the Effective Date is [AGREEMENT_HASH].

---

## ARTICLE II: MEMBERSHIP

### Section 2.1 -- Membership Eligibility

(a) Membership in the Company shall be determined by the holding of Governance Tokens, as defined in Section 1.9(c) of this Agreement.

(b) Any person or entity that holds one (1) or more Governance Tokens shall be deemed a Member of the Company and shall be bound by the terms of this Agreement.

(c) Membership in the Company is open to any natural person, legal entity, trust, or other organization that is not a Restricted Person as defined in Section 5.2 of this Agreement.

(d) By acquiring Governance Tokens, each holder agrees to be bound by the terms and conditions of this Agreement without the need for a separate signature. The act of holding Governance Tokens in a Solana wallet constitutes acceptance of this Agreement.

### Section 2.2 -- Governance Tokens

(a) The Governance Tokens are **Token-2022 SPL tokens** deployed on the Solana blockchain at mint address [TOKEN_MINT].

(b) The maximum supply of Governance Tokens authorized for issuance is [MAX_TOKEN_SUPPLY] tokens (with [TOKEN_DECIMALS] decimal places).

(c) As of the Effective Date, [INITIAL_TOKEN_SUPPLY] Governance Tokens have been issued and are outstanding.

(d) Additional Governance Tokens may only be issued pursuant to a proposal approved through the Governance Program in accordance with Article III. The mint authority is held by the Squads Vault PDA at [MULTISIG_ADDRESS], requiring multisig approval for all minting operations.

### Section 2.3 -- Membership Interests

(a) Each Governance Token represents one (1) unit of Membership Interest in the Company.

(b) Membership Interests shall entitle each Member to:

   (i) Voting rights proportional to the number of Governance Tokens held, subject to the terms of Article III;

   (ii) The right to participate in distributions of the Company's net profits and revenues, proportional to the number of Governance Tokens held relative to the total outstanding Governance Tokens, subject to the terms of Article VII;

   (iii) The right to receive information regarding the Company's operations and financial condition, subject to applicable confidentiality provisions;

   (iv) The right to submit proposals for consideration through the Governance Program;

   (v) Such other rights as are provided in this Agreement or by applicable law.

(c) No Member shall have any interest in specific Company property. The Company's assets shall be owned by the Company as a legal entity separate from its Members.

### Section 2.4 -- On-Chain Member Registry

(a) The Company's membership registry (the "Member Registry") shall be maintained exclusively through the Smart Contract Code on the Solana blockchain. The on-chain Member Registry consists of:

   (i) Token-2022 token accounts holding Governance Tokens at mint [TOKEN_MINT], which record each Member's wallet address and token balance;

   (ii) MemberRecord PDAs maintained by the Cap Table Program at [CAP_TABLE_PROGRAM_ADDRESS], which store KYC verification hashes and member status.

(b) The on-chain Member Registry satisfies the member list requirement under the LLC Act. No duplicate off-chain membership registry shall be required.

(c) The blockchain record shall be the authoritative source for determining:

   (i) Whether a person or entity is a Member of the Company;

   (ii) The number of Governance Tokens held by each Member;

   (iii) The proportionate Membership Interest of each Member;

   (iv) The date on which a person became or ceased to be a Member (determinable from Solana transaction history).

(d) Membership status shall automatically update upon the transfer, acquisition, or disposition of Governance Tokens on the Solana blockchain. No further action by the Company or any Member shall be required to effectuate a change in membership status.

### Section 2.5 -- Token Transfer and Membership Transfer

(a) Governance Tokens may be freely transferred on the Solana blockchain, subject to:

   (i) Any transfer restrictions imposed by the Smart Contract Code, including but not limited to the Transfer Hook program at address [TRANSFER_HOOK_ADDRESS], which enforces KYC verification, sanctions screening, accredited investor checks, and lock-up periods on every transfer;

   (ii) Applicable securities laws of any jurisdiction in which the transferor or transferee is located;

   (iii) Applicable anti-money laundering and sanctions laws;

   (iv) Any lock-up periods established by the Governance Program or encoded in vesting schedules.

(b) Transfer of Governance Tokens shall automatically and simultaneously transfer the corresponding Membership Interest, including all associated voting rights, economic rights, and obligations.

(c) No Member shall be required to obtain the consent of the Company or any other Member to transfer Governance Tokens, except as expressly provided in this Agreement or enforced by the Smart Contract Code.

### Section 2.6 -- KYC and Compliance

(a) Members holding twenty-five percent (25%) or more of the total outstanding Governance Tokens, or otherwise exercising control over the Company, shall be required to complete Know Your Customer ("KYC") verification, including submission of:

   (i) Full legal name;

   (ii) Date of birth;

   (iii) Residential address;

   (iv) Nonexpired government-issued identification (passport);

   (v) All Solana wallet address(es) associated with the Company.

(b) Members holding between ten percent (10%) and twenty-five percent (25%) of the total outstanding Governance Tokens shall complete KYC verification as required by the local regulator.

(c) Members holding less than ten percent (10%) of the total outstanding Governance Tokens may remain anonymous, subject to applicable law.

(d) The Company shall designate at least one Ultimate Beneficial Owner ("UBO") who shall be identified and undergo KYC screening in accordance with the DAO Regulations of 2024. The UBO's information shall be included in the Beneficial Owner Information Report filed with the Registrar.

(e) KYC verification shall be renewed annually, typically in January of each year.

(f) A Member's KYC status shall be recorded on-chain via a cryptographic hash stored in the Member's MemberRecord PDA, with actual KYC documentation maintained off-chain by the Company's designated compliance provider. The `kyc_verified` field in the MemberRecord PDA shall be set to `true` upon successful verification.

### Section 2.7 -- Initial Members

The initial Members of the Company as of the Effective Date, together with their respective Governance Token holdings, are set forth in Exhibit A attached hereto.

### Section 2.8 -- Voting Delegation

(a) Any Member may delegate their voting power to another Member or to any Solana wallet address by executing a delegation transaction through the Governance Program.

(b) Delegation of voting power does not transfer Membership Interest, economic rights, or any other rights under this Agreement other than the specific right to vote on the delegating Member's behalf.

(c) Delegation may be revoked at any time by the delegating Member through the Governance Program.

(d) A delegate may not further delegate voting power received from another Member unless expressly authorized by the Governance Program.

---

## ARTICLE III: GOVERNANCE

### Section 3.1 -- Governance Model

(a) The Company shall be **[CHOOSE: member-managed / algorithmically managed]** as designated in the Certificate of Formation.

(b) Governance of the Company shall be conducted through the on-chain Governance Program deployed at address [PROGRAM_ADDRESS] on the Solana blockchain, supplemented by the provisions of this Agreement.

(c) The Governance Program implements governance through the **Squads Protocol v4** multisig deployed at address [MULTISIG_ADDRESS], which serves as the primary mechanism for executing governance decisions, managing the Company treasury, and administering the Company's smart contracts.

### Section 3.2 -- Voting Rights

(a) Each Governance Token entitles its holder to one (1) vote on any matter submitted to the Members for a vote through the Governance Program.

(b) Voting power is calculated as the number of Governance Tokens held by a Member (or delegated to such Member) at the time a vote is cast, divided by the total number of outstanding Governance Tokens. Voting power is calculated on-chain from Token-2022 token account balances weighted by share class (if applicable).

(c) Voting shall be conducted on-chain through the Governance Program, except as otherwise provided in this Agreement for emergency or off-chain proposals.

### Section 3.3 -- Proposal Types and Approval Thresholds

The following proposal types and associated approval thresholds shall apply:

| Proposal Type | Quorum Requirement | Approval Threshold | Voting Period |
|--------------|-------------------|-------------------|---------------|
| Ordinary Proposal | [ORDINARY_QUORUM]% of outstanding tokens | Simple majority (>50%) of votes cast | [ORDINARY_VOTING_PERIOD] days |
| Treasury Disbursement (below [TREASURY_THRESHOLD]) | [TREASURY_QUORUM]% of outstanding tokens | Simple majority (>50%) of votes cast | [TREASURY_VOTING_PERIOD] days |
| Treasury Disbursement (at or above [TREASURY_THRESHOLD]) | [LARGE_TREASURY_QUORUM]% of outstanding tokens | [LARGE_TREASURY_APPROVAL]% supermajority of votes cast | [LARGE_TREASURY_VOTING_PERIOD] days |
| Token Issuance | [ISSUANCE_QUORUM]% of outstanding tokens | [ISSUANCE_APPROVAL]% supermajority of votes cast | [ISSUANCE_VOTING_PERIOD] days |
| Amendment to Operating Agreement | [AMENDMENT_QUORUM]% of outstanding tokens | [AMENDMENT_APPROVAL]% supermajority of votes cast | [AMENDMENT_VOTING_PERIOD] days |
| Dissolution | [DISSOLUTION_QUORUM]% of outstanding tokens | [DISSOLUTION_APPROVAL]% supermajority of votes cast | [DISSOLUTION_VOTING_PERIOD] days |
| Smart Contract Upgrade | [UPGRADE_QUORUM]% of outstanding tokens | [UPGRADE_APPROVAL]% supermajority of votes cast | [UPGRADE_VOTING_PERIOD] days |
| [ADDITIONAL_PROPOSAL_TYPE] | [ADDITIONAL_QUORUM]% | [ADDITIONAL_APPROVAL]% | [ADDITIONAL_VOTING_PERIOD] days |

> **[NOTE: Typical starting values -- Ordinary: 10% quorum, 50% approval, 5 days. Treasury (large): 15% quorum, 66% approval, 7 days. Token issuance: 20% quorum, 66% approval, 7 days. Amendment: 20% quorum, 75% approval, 10 days. Dissolution: 33% quorum, 75% approval, 14 days. Adjust based on DAO size and activity level.]**

### Section 3.4 -- Proposal Submission

(a) Any Member holding at least [MINIMUM_PROPOSAL_TOKENS] Governance Tokens may submit a proposal through the Governance Program.

(b) Each proposal shall include:

   (i) A clear description of the proposed action;

   (ii) The proposal type, as defined in Section 3.3;

   (iii) Any on-chain transactions (Solana instructions) to be executed upon approval;

   (iv) The rationale for the proposed action.

(c) A proposal shall become active and open for voting upon submission to the Governance Program, subject to any review or cooling-off period established by the Governance Program.

(d) Proposals are recorded on-chain as Proposal PDAs, with the proposal text hash and vote tallies stored immutably on the Solana ledger.

### Section 3.5 -- Voting Procedure

(a) Once a proposal is active, Members may cast their votes through the Governance Program during the applicable Voting Period.

(b) Each Member may vote "For," "Against," or "Abstain" on any active proposal.

(c) Abstentions shall be counted toward quorum but shall not be counted toward the approval threshold.

(d) A proposal shall be deemed approved if, at the conclusion of the Voting Period:

   (i) The applicable quorum requirement has been met; and

   (ii) The number of "For" votes exceeds the applicable approval threshold.

(e) Approved proposals shall be executed through the Governance Program or the Squads Protocol multisig, as applicable. Execution of approved on-chain transactions occurs atomically via the Squads multisig instruction execution flow.

(f) All votes are recorded on-chain in Vote PDAs, creating an immutable audit trail of governance decisions on the Solana ledger.

### Section 3.6 -- Off-Chain Proposals

(a) Where a matter cannot be practically submitted to the on-chain Governance Program (including but not limited to matters requiring legal execution, regulatory filings, or actions outside the scope of the Smart Contract Code), an off-chain proposal may be submitted.

(b) Off-chain proposals shall be documented in writing, distributed to all Members through the Company's designated communication channels, and ratified through an on-chain vote.

(c) No off-chain proposal shall be binding on the Company unless ratified through an on-chain vote in accordance with the applicable approval thresholds set forth in Section 3.3.

### Section 3.7 -- Multisig Governance

(a) The Squads Protocol v4 multisig at address [MULTISIG_ADDRESS] shall operate with a threshold of [MULTISIG_THRESHOLD] of [MULTISIG_TOTAL] signers.

(b) The initial multisig signers are set forth in Exhibit B attached hereto.

(c) Multisig signers may be added, removed, or replaced, and the approval threshold may be modified, through a proposal approved in accordance with Section 3.3.

(d) All multisig transactions involving the Company treasury or smart contract administration shall be subject to a time lock of [TIMELOCK_PERIOD] hours between approval and execution, except in the case of Emergency Actions as defined in Section 3.8.

(e) The Squads multisig manages the following Vault PDAs:

   (i) **Vault 0** (Default Treasury): [VAULT_0_ADDRESS] -- Primary treasury holding SOL, USDC, and other digital assets;

   (ii) **Vault 1** (Program Upgrade Authority): [VAULT_1_ADDRESS] -- Upgrade authority for the Company's Solana programs;

   (iii) **Vault 2** (Entity Authority): [VAULT_2_ADDRESS] -- Authority on the Entity PDA for cap table operations;

   (iv) [ADDITIONAL_VAULT_DESCRIPTION].

### Section 3.8 -- Emergency Actions

(a) In the event of an imminent threat to the Company's assets, smart contracts, or operational integrity (including but not limited to smart contract exploits, hacks, private key compromises, or critical vulnerabilities), the multisig signers may take Emergency Actions without prior approval through the Governance Program.

(b) Emergency Actions are limited to:

   (i) Pausing or freezing the Company's smart contracts, including invoking freeze instructions on the Governance Token mint;

   (ii) Executing emergency smart contract upgrades to patch critical vulnerabilities;

   (iii) Transferring assets to a secure Solana address to prevent loss.

(c) Any Emergency Action taken pursuant to this Section 3.8 must be reported to the Members through the Company's designated communication channels within [EMERGENCY_REPORT_PERIOD] hours and shall be subject to ratification through the Governance Program within [EMERGENCY_RATIFICATION_PERIOD] days.

(d) Emergency Actions shall not include the issuance of new Governance Tokens, amendments to this Agreement, or dissolution of the Company.

---

## ARTICLE IV: MANAGEMENT

### Section 4.1 -- Management Designation

The Company shall be **[CHOOSE: member-managed / algorithmically managed]**.

(a) If member-managed: The Members, acting through the Governance Program and the Squads Protocol multisig, shall have the authority to manage the business and affairs of the Company.

(b) If algorithmically managed: The Smart Contract Code deployed on the Solana blockchain shall manage the routine operations of the Company, with Members retaining authority over non-routine matters through the Governance Program.

### Section 4.2 -- Special Delegates

(a) The Company may appoint one or more Special Delegates to exercise specific administrative, operational, or representational powers on behalf of the Company.

(b) Special Delegates are not officers, directors, or managers of the Company. They are agents with limited, specifically delegated authority.

(c) The initial Special Delegates, together with their delegated powers, are set forth in Exhibit C attached hereto.

(d) Special Delegates may be appointed, removed, or their authority modified through a proposal approved in accordance with Section 3.3.

(e) The powers of a Special Delegate may include, but are not limited to:

   (i) Executing contracts, agreements, and instruments on behalf of the Company within the scope of their delegated authority;

   (ii) Managing the Company's financial accounts and banking relationships;

   (iii) Administering the Company's AML/KYC compliance program;

   (iv) Representing the Company before governmental authorities and regulators;

   (v) Managing the Company's day-to-day operations within parameters established by the Governance Program;

   (vi) Filing annual reports, tax returns, and other regulatory documents on behalf of the Company;

   (vii) Managing the Company's Solana validator or staking operations (if applicable).

(f) A Special Delegate shall not have authority to:

   (i) Amend this Operating Agreement;

   (ii) Issue or authorize the issuance of Governance Tokens;

   (iii) Dissolve the Company;

   (iv) Enter into transactions outside the scope of their specifically delegated authority;

   (v) Bind the Company to obligations exceeding [DELEGATE_SPENDING_LIMIT] without prior approval through the Governance Program.

### Section 4.3 -- Councils

(a) The Members may, through the Governance Program, establish one or more Councils to oversee specific areas of the Company's operations.

(b) Each Council shall operate pursuant to a Council Charter approved through the Governance Program, which shall define:

   (i) The Council's name, purpose, and scope of authority;

   (ii) The number of Council members and their selection process;

   (iii) The Council's decision-making procedures;

   (iv) The Council's term and renewal process;

   (v) The Council's reporting obligations to the Members.

(c) Councils shall have advisory authority only, unless the Council Charter expressly grants executive authority for specific matters.

### Section 4.4 -- AML/KYC Compliance

(a) The Company shall adopt and maintain an Anti-Money Laundering and Know Your Customer ("AML/KYC") compliance program consistent with the requirements of the DAO Regulations and applicable international standards.

(b) The AML/KYC compliance program shall include, at minimum:

   (i) Screening of Members holding twenty-five percent (25%) or more of Governance Tokens against applicable sanctions lists (FATF, UN Security Council, HMT, US OFAC, EU);

   (ii) Monitoring of cross-border transactions exceeding USD 1,000;

   (iii) Annual KYC renewal for applicable Members;

   (iv) Procedures for identifying and removing Restricted Persons;

   (v) Real-time monitoring of the 25% governance threshold based on on-chain token balances, with automatic KYC triggers when a wallet address crosses the threshold.

(c) Responsibility for administering the AML/KYC compliance program may be delegated to a Special Delegate or external compliance provider.

### Section 4.5 -- Banking and Financial Accounts

(a) The Company may open and maintain bank accounts and financial accounts with such institutions as the Members approve through the Governance Program or as authorized Special Delegates determine.

(b) No local bank account in the Marshall Islands is required.

(c) The Company may hold and manage digital assets, including but not limited to SOL, USDC, and other SPL tokens on the Solana blockchain, in wallets controlled by the Squads Protocol multisig or through other mechanisms approved by the Members.

---

## ARTICLE V: DISSOCIATION

### Section 5.1 -- Automatic Dissociation

(a) A Member shall be automatically dissociated from the Company (i.e., cease to be a Member) when such Member's Governance Token balance reaches zero (0) as recorded on the Solana blockchain.

(b) Dissociation pursuant to this Section 5.1 shall be effective immediately upon the on-chain transaction that causes the Member's Governance Token balance to reach zero, without further action by the Company or any other Member.

(c) Upon automatic dissociation, the former Member shall have no further rights, obligations, or liabilities as a Member of the Company, except for:

   (i) Any rights to distributions that were declared but unpaid prior to dissociation;

   (ii) Any obligations or liabilities arising from the former Member's actions or omissions during the period of membership;

   (iii) Any rights or obligations that survive termination of membership under this Agreement or applicable law.

### Section 5.2 -- Restricted Persons

(a) A "Restricted Person" is any person or entity that:

   (i) Appears on any sanctions list maintained by the Financial Action Task Force (FATF), the United Nations Security Council, Her Majesty's Treasury (HMT), the United States Office of Foreign Assets Control (OFAC), the European Union, or any other applicable sanctions authority;

   (ii) Is located in, organized under, or a resident of a jurisdiction subject to comprehensive sanctions by the United States, the European Union, or the United Nations;

   (iii) Has been found to be in violation of the Company's adopted AML policies;

   (iv) Is otherwise prohibited from participating in the Company under applicable law.

(b) Any Member who becomes a Restricted Person shall be immediately and automatically dissociated from the Company.

(c) The Smart Contract Code may include transfer restrictions (including but not limited to the Transfer Hook program at [TRANSFER_HOOK_ADDRESS]) designed to prevent Restricted Persons from acquiring or holding Governance Tokens. Such restrictions are supplemental to, and do not limit, the Company's rights under this Section 5.2.

(d) The Company, acting through its Special Delegates or the Squads Protocol multisig, may take such action as is necessary to enforce the provisions of this Section, including but not limited to:

   (i) Utilizing the Permanent Delegate extension of the Token-2022 program to effect forced transfers or burns of Governance Tokens held by Restricted Persons;

   (ii) Blocking wallet addresses associated with Restricted Persons via the Transfer Hook program;

   (iii) Reporting suspected sanctions violations to applicable authorities.

### Section 5.3 -- Voluntary Withdrawal

(a) Any Member may voluntarily withdraw from the Company at any time by transferring or disposing of all of their Governance Tokens.

(b) A Member who voluntarily withdraws shall not be entitled to receive any distribution or payment from the Company on account of such withdrawal, except for any distributions that were declared but unpaid prior to withdrawal.

### Section 5.4 -- Involuntary Removal

(a) A Member may be involuntarily removed from the Company by a proposal approved through the Governance Program in accordance with the approval thresholds for an Ordinary Proposal as set forth in Section 3.3.

(b) Involuntary removal may be proposed on the grounds that a Member:

   (i) Has materially breached this Agreement;

   (ii) Has engaged in conduct that is materially adverse to the interests of the Company;

   (iii) Has become a Restricted Person;

   (iv) Has failed to comply with applicable KYC requirements after reasonable notice and opportunity to cure.

(c) Upon approval of a removal proposal, the Company may utilize the Permanent Delegate extension of the Token-2022 program to effect the transfer or redemption of the removed Member's Governance Tokens, with such Member receiving fair market value therefor as determined by the Governance Program or an independent appraiser.

---

## ARTICLE VI: LIABILITY AND FIDUCIARY DUTIES

### Section 6.1 -- Limited Liability

(a) No Member shall be liable, as such, for the liabilities of the Company. The failure of the Company to observe any formalities or requirements relating to the exercise of its powers or the management of its business or affairs under this Agreement or the LLC Act shall not be grounds for imposing personal liability on any Member for liabilities of the Company.

(b) The debts, obligations, and liabilities of the Company, whether arising in contract, tort, or otherwise, shall be solely the debts, obligations, and liabilities of the Company, and no Member shall be obligated personally for any such debt, obligation, or liability solely by reason of being or acting as a Member of the Company.

(c) Each Member's liability to the Company shall be limited to the amount of such Member's capital contribution (if any) and the value of the Governance Tokens held by such Member. No Member shall be required to make any additional capital contributions beyond the acquisition price of their Governance Tokens.

(d) The liability protections afforded by this Section 6.1 shall be interpreted in accordance with Delaware precedent regarding LLC liability shields, as incorporated into Marshall Islands law.

### Section 6.2 -- Waiver of Fiduciary Duties

(a) THIS AGREEMENT IS NOT INTENDED TO, AND DOES NOT, CREATE OR IMPOSE ANY FIDUCIARY DUTY ON ANY MEMBER. EACH MEMBER HEREBY WAIVES, TO THE FULLEST EXTENT PERMITTED BY APPLICABLE LAW, ANY AND ALL FIDUCIARY DUTIES THAT, ABSENT SUCH WAIVER, MAY BE IMPLIED BY LAW.

(b) The Members acknowledge that the Company is a decentralized autonomous organization in which governance is exercised collectively through on-chain voting, and that the traditional concepts of fiduciary duty are inconsistent with the Company's decentralized governance model.

(c) Notwithstanding the foregoing, each Member shall be subject to the implied covenant of good faith and fair dealing, which covenant may not be waived under Marshall Islands law.

### Section 6.3 -- Implied Covenant of Good Faith

(a) Each Member shall act in good faith and deal fairly with the Company and with the other Members.

(b) The implied covenant of good faith and fair dealing shall be interpreted consistently with the decentralized nature of the Company and shall not be construed to impose upon any Member any duty greater than that of refraining from bad faith conduct.

### Section 6.4 -- Indemnification

(a) The Company shall indemnify and hold harmless each Member, each Special Delegate, and each Council member (each, an "Indemnified Person") from and against any loss, damage, liability, cost, or expense (including reasonable attorneys' fees) arising out of or related to such Indemnified Person's good faith actions taken on behalf of the Company or in furtherance of the Company's purposes, to the fullest extent permitted by the LLC Act.

(b) The Company shall not indemnify any Indemnified Person for losses resulting from such person's:

   (i) Willful misconduct or fraud;

   (ii) Knowing violation of law;

   (iii) Actions taken in bad faith or with reckless disregard for the Company's interests;

   (iv) Self-dealing transactions not approved through the Governance Program.

(c) The Company may advance defense costs to an Indemnified Person prior to the final disposition of a proceeding, upon receipt of an undertaking by such person to repay such amounts if it is ultimately determined that such person is not entitled to indemnification.

(d) The Company may purchase and maintain insurance on behalf of any Indemnified Person against any liability asserted against such person in connection with the Company's activities, whether or not the Company would have the power to indemnify such person against such liability under this Agreement.

### Section 6.5 -- Open-Source Software Immunity

Pursuant to the DAO Act Amendments of 2023, the Company shall not be liable for any claim arising from the use, modification, or distribution of open-source software created or contributed to by the Company, including but not limited to Solana programs, Anchor smart contracts, and associated tooling, and including claims arising from third-party misuse of such software. This immunity extends to all Members, Special Delegates, and other agents of the Company acting in their capacity as such.

---

## ARTICLE VII: FINANCIAL PROVISIONS

### Section 7.1 -- Capital Contributions

(a) No Member shall be required to make any capital contribution to the Company beyond the acquisition price of their Governance Tokens.

(b) The Company may accept additional capital contributions from Members or third parties as approved through the Governance Program.

(c) Capital contributions may be made in fiat currency, digital assets (including SOL, USDC, or other SPL tokens), or other property acceptable to the Company.

### Section 7.2 -- Treasury Management

(a) The Company's treasury shall be managed through the Squads Protocol v4 multisig at address [MULTISIG_ADDRESS], utilizing separate Vault PDAs for different operational purposes.

(b) The Company may maintain treasury assets in various forms, including but not limited to:

   (i) SOL, USDC, and other SPL tokens on the Solana blockchain;

   (ii) Fiat currency in bank accounts;

   (iii) Digital assets on other blockchains (bridged or held in separate wallets);

   (iv) Other property as approved by the Members.

(c) Treasury transactions exceeding [TREASURY_THRESHOLD] in value shall require approval through the Governance Program as a Treasury Disbursement proposal.

(d) Squads spending limits may be established for individual multisig signers, allowing routine operational expenditures within defined limits without requiring full multisig approval.

### Section 7.3 -- Profit Distribution

(a) The Company may distribute net profits and revenues to Members in proportion to their respective Membership Interests (i.e., in proportion to the number of Governance Tokens held by each Member relative to the total outstanding Governance Tokens at the time of distribution).

(b) Distributions shall be declared and authorized through a proposal approved by the Governance Program in accordance with Section 3.3.

(c) Distributions may be made in any form of consideration, including but not limited to SOL, USDC, other SPL tokens, fiat currency, or other property.

(d) No distribution shall be made if, after giving effect to the distribution:

   (i) The Company would be unable to pay its debts as they become due in the ordinary course of business; or

   (ii) The Company's total assets would be less than the sum of its total liabilities.

(e) Distributions shall be executed through the Squads Protocol multisig or other mechanisms approved by the Governance Program.

### Section 7.4 -- Taxation

(a) The Company is subject to the Marshall Islands Gross Revenue Tax ("GRT") at a rate of three percent (3%) on earned revenue and interest income. Capital gains and dividends are excluded from the GRT.

(b) The Company shall not pass through income, gains, losses, deductions, or credits to its Members for tax purposes. Members are not individually liable for the Company's tax obligations solely by reason of membership.

(c) Each Member acknowledges that they are solely responsible for their own tax obligations arising in their respective jurisdictions of residence or citizenship with respect to distributions received from the Company or the appreciation or depreciation in value of their Governance Tokens.

(d) The Company shall file all required tax returns and pay all taxes due in a timely manner. Responsibility for tax compliance may be delegated to a Special Delegate or external tax advisor.

### Section 7.5 -- Financial Reporting

(a) The Company shall prepare and maintain financial records sufficient to comply with the DAO Regulations and applicable tax law.

(b) Annual financial reports shall be made available to Members no later than [FINANCIAL_REPORT_DEADLINE] of each year.

(c) Financial records may be maintained on-chain, off-chain, or a combination thereof, provided that they are accurate, complete, and accessible to Members upon reasonable request. On-chain treasury transactions are inherently recorded on the Solana ledger and serve as an immutable audit trail.

---

## ARTICLE VIII: AMENDMENTS

### Section 8.1 -- Amendment Process

(a) This Agreement may be amended only by a proposal approved through the Governance Program in accordance with the approval thresholds for an Amendment to Operating Agreement as set forth in Section 3.3.

(b) No amendment shall be effective until:

   (i) The proposal has been approved in accordance with Section 3.3;

   (ii) The amended text has been documented and made available to all Members;

   (iii) The on-chain Charter Hash stored in the Entity PDA has been updated to reflect the SHA-256 hash of the amended Agreement.

### Section 8.2 -- Electronic Execution

(a) This Agreement and any amendments hereto may be executed by electronic means, including by cryptographic wallet signature on the Solana blockchain.

(b) A Member's vote "For" an amendment proposal through the Governance Program shall constitute that Member's agreement to and execution of the amendment, with the same legal effect as a physical signature.

(c) The Solana transaction signature (hash) of the approved amendment proposal shall serve as evidence of execution.

### Section 8.3 -- Smart Contract Upgrades

(a) Material modifications to the Smart Contract Code referenced in Section 1.9 shall require approval through the Governance Program as a Smart Contract Upgrade proposal in accordance with Section 3.3.

(b) All smart contract upgrades shall be subject to:

   (i) A time lock of [UPGRADE_TIMELOCK_PERIOD] hours between approval and execution;

   (ii) Public disclosure of the proposed changes prior to the vote, including verifiable build artifacts;

   (iii) Verification that the deployed code matches the publicly disclosed changes (using Solana Verify or equivalent tools).

(c) Material changes to the Smart Contract Code may require re-submission of technical documentation to the Registrar of Corporations and disclosure in the Company's next annual report.

(d) The Squads Protocol multisig serves as the upgrade authority for the Company's Solana programs. All program upgrades require multisig approval with the threshold specified in Section 3.7. The upgrade flow is:

   (i) Developer deploys new program buffer via `solana program write-buffer`;

   (ii) Upgrade proposal is created in the Squads multisig;

   (iii) Required threshold of signers approve the proposal;

   (iv) After the time lock period, the proposal is executed, atomically replacing the program code.

### Section 8.4 -- Non-Amendable Provisions

The following provisions of this Agreement may not be amended except by unanimous consent of all Members:

(a) The limitation of liability set forth in Section 6.1;

(b) The indemnification provisions set forth in Section 6.4;

(c) The governing law provision set forth in Section 1.3;

(d) This Section 8.4.

---

## ARTICLE IX: DISSOLUTION

### Section 9.1 -- Dissolution Events

The Company shall be dissolved upon the occurrence of any of the following events:

(a) A proposal to dissolve the Company is approved through the Governance Program in accordance with the dissolution approval thresholds set forth in Section 3.3;

(b) The entry of a decree of judicial dissolution under the LLC Act;

(c) The cancellation of the Company's Certificate of Formation by the Registrar of Corporations for persistent non-compliance with applicable law or regulations;

(d) The occurrence of any event that makes it unlawful for the Company to continue its business.

### Section 9.2 -- Winding Up

(a) Upon dissolution, the Company's affairs shall be wound up by the Special Delegates or, if no Special Delegates are then serving, by a liquidating agent appointed through the Governance Program or by a court of competent jurisdiction.

(b) During the winding up period, the Company shall:

   (i) Cease conducting business except to the extent necessary for the orderly winding up of its affairs;

   (ii) Collect all amounts owed to the Company;

   (iii) Pay or make adequate provision for all debts, liabilities, and obligations of the Company;

   (iv) Distribute any remaining assets to the Members.

### Section 9.3 -- Distribution Upon Dissolution

(a) After payment or adequate provision for all debts, liabilities, and obligations of the Company, remaining assets shall be distributed to the Members in proportion to their respective Membership Interests (i.e., in proportion to the number of Governance Tokens held by each Member at the time of dissolution).

(b) Distributions upon dissolution may be made in SOL, USDC, other SPL tokens, fiat currency, or in kind, as determined by the person or persons conducting the winding up.

(c) Distributions shall be executed through the Squads Protocol multisig or other mechanisms approved by the Governance Program.

### Section 9.4 -- Certificate of Cancellation

Upon the completion of the winding up and distribution of assets, the Company shall file a Certificate of Cancellation with the Registrar of Corporations of the Republic of the Marshall Islands.

---

## ARTICLE X: DISPUTE RESOLUTION

### Section 10.1 -- Negotiation

(a) In the event of any dispute, claim, or controversy arising out of or relating to this Agreement, the Company, or any Member's rights or obligations hereunder (a "Dispute"), the parties to such Dispute shall first attempt to resolve the Dispute through good faith negotiation.

(b) The negotiation period shall be thirty (30) days from the date on which a party provides written notice to the other party or parties of the Dispute (the "Dispute Notice").

(c) For the purposes of this Section, "written notice" includes electronic notice sent to the Solana wallet address of a Member, to a Member's email address on file, or through the Company's designated communication channels.

### Section 10.2 -- Arbitration

(a) If the Dispute is not resolved through negotiation within the thirty (30) day period specified in Section 10.1, any party to the Dispute may submit the Dispute to binding arbitration administered by the **International Centre for Dispute Resolution** ("ICDR"), a division of the American Arbitration Association, in accordance with the ICDR International Arbitration Rules then in effect.

(b) The arbitration shall be conducted in the Republic of the Marshall Islands, or at such other location as the parties may agree or the arbitral tribunal may determine.

(c) The arbitral tribunal shall consist of [CHOOSE: one (1) / three (3)] arbitrator(s) selected in accordance with the ICDR International Arbitration Rules.

(d) The language of the arbitration shall be English.

(e) The arbitral tribunal shall apply the laws of the Republic of the Marshall Islands as specified in Section 1.3.

(f) The decision of the arbitral tribunal shall be final and binding upon the parties, and judgment upon the award rendered by the arbitral tribunal may be entered in any court of competent jurisdiction.

### Section 10.3 -- Costs

Each party to an arbitration shall bear its own costs and expenses (including attorneys' fees), unless the arbitral tribunal determines that a different allocation is appropriate.

### Section 10.4 -- Exclusion of Court Jurisdiction

(a) To the fullest extent permitted by applicable law, the parties waive any right to bring any Dispute arising under or related to this Agreement before any court of law or equity, except for the enforcement of an arbitral award or emergency injunctive relief.

(b) Nothing in this Article X shall prevent any party from seeking interim or emergency relief from a court of competent jurisdiction to prevent irreparable harm pending the constitution of the arbitral tribunal.

### Section 10.5 -- Class Action Waiver

TO THE FULLEST EXTENT PERMITTED BY APPLICABLE LAW, EACH MEMBER WAIVES THE RIGHT TO PARTICIPATE IN A CLASS ACTION, COLLECTIVE ACTION, OR REPRESENTATIVE ACTION WITH RESPECT TO ANY DISPUTE ARISING UNDER OR RELATED TO THIS AGREEMENT. ALL DISPUTES SHALL BE RESOLVED ON AN INDIVIDUAL BASIS.

---

## ARTICLE XI: RECORDS AND REPORTING

### Section 11.1 -- On-Chain Records

(a) The on-chain Member Registry maintained through the Smart Contract Code on the Solana blockchain satisfies the requirement under the LLC Act to maintain a current list of the full name and last known mailing address or blockchain identifier of each Member.

(b) The following records shall be maintained on-chain and shall constitute the authoritative record for the matters they address:

   (i) Member Registry (Governance Token balances at mint [TOKEN_MINT] and MemberRecord PDAs);

   (ii) Governance proposals and voting records (Proposal and Vote PDAs);

   (iii) Treasury transactions executed through the Squads Protocol multisig;

   (iv) Smart contract code and deployment addresses;

   (v) Operating Agreement hash (SHA-256) stored in the Entity PDA.

### Section 11.2 -- Off-Chain Records

(a) The Company shall maintain the following records off-chain, which shall be accessible to Members upon reasonable request:

   (i) The full text of this Operating Agreement and all amendments;

   (ii) The Certificate of Formation and any amendments;

   (iii) Financial statements and tax returns;

   (iv) KYC documentation (maintained in accordance with applicable privacy laws);

   (v) Minutes or records of any off-chain proceedings;

   (vi) Any contracts, agreements, or instruments executed by or on behalf of the Company.

(b) Off-chain records may be stored on decentralized storage systems (including but not limited to IPFS or Arweave), provided that their integrity can be verified by comparison with on-chain hashes.

### Section 11.3 -- Annual Reporting

(a) The Company shall file an annual report with the Registrar of Corporations of the Republic of the Marshall Islands between January 1st and March 31st of each year, as required by the DAO Regulations.

(b) The annual report shall include:

   (i) Updated beneficial ownership information;

   (ii) Leadership and management details;

   (iii) Community engagement summary;

   (iv) Financial information, including GRT calculation and payment records;

   (v) Confirmation of operational status;

   (vi) Any structural changes during the preceding year;

   (vii) Any material changes to the Company's smart contracts, including program upgrades and new deployments.

### Section 11.4 -- Beneficial Ownership Reporting

(a) The Company shall maintain and file a Beneficial Owner Information Report ("BOIR") as required by the DAO Regulations.

(b) The BOIR shall be updated promptly if beneficial ownership information changes, including when a new Member crosses the twenty-five percent (25%) governance threshold as determined by on-chain token balances.

### Section 11.5 -- Confidentiality

(a) Each Member shall maintain the confidentiality of non-public information regarding the Company's operations, finances, and strategies that is disclosed to such Member in the Member's capacity as a Member.

(b) The obligation of confidentiality shall not apply to information that:

   (i) Is or becomes publicly available through no breach of this Agreement;

   (ii) Is independently developed by the Member without reference to the Company's confidential information;

   (iii) Is required to be disclosed by applicable law, regulation, or court order;

   (iv) Is disclosed on-chain through the Company's smart contracts and governance processes.

---

## ARTICLE XII: GENERAL PROVISIONS

### Section 12.1 -- Entire Agreement

This Agreement, together with the Exhibits attached hereto, constitutes the entire agreement among the Members with respect to the subject matter hereof and supersedes all prior and contemporaneous agreements, understandings, negotiations, and discussions, whether oral or written, relating to such subject matter.

### Section 12.2 -- Severability

If any provision of this Agreement is held to be invalid, illegal, or unenforceable in any respect, such invalidity, illegality, or unenforceability shall not affect any other provision, and this Agreement shall be construed as if such invalid, illegal, or unenforceable provision had never been contained herein.

### Section 12.3 -- Waiver

The failure of any Member to enforce any provision of this Agreement shall not constitute a waiver of such Member's right to enforce such provision or any other provision in the future.

### Section 12.4 -- Binding Effect

This Agreement shall be binding upon and shall inure to the benefit of the Members and their respective successors, assigns, heirs, and legal representatives.

### Section 12.5 -- Notices

All notices required or permitted under this Agreement shall be in writing and shall be deemed delivered when:

(a) Sent to a Member's Solana wallet address through an on-chain transaction or message;

(b) Sent to a Member's email address on file with the Company;

(c) Published on the Company's designated communication channels (including but not limited to [COMMUNICATION_CHANNELS]).

### Section 12.6 -- Interpretation

(a) The headings in this Agreement are for convenience of reference only and shall not affect the interpretation of this Agreement.

(b) References to "including" mean "including without limitation."

(c) References to "applicable law" include all applicable statutes, regulations, rules, orders, and judicial decisions.

(d) All references to blockchain addresses are to addresses on the Solana blockchain (Mainnet-Beta) unless otherwise specified.

(e) All references to "on-chain" refer to the Solana blockchain unless otherwise specified.

### Section 12.7 -- Counterparts

This Agreement may be executed in counterparts, including by electronic or cryptographic signature (including Ed25519 digital signatures on the Solana blockchain), each of which shall be deemed an original and all of which together shall constitute one and the same instrument.

### Section 12.8 -- No Third-Party Beneficiaries

This Agreement is entered into solely for the benefit of the Members and the Company and shall not confer any rights upon any third party, except for the indemnification rights granted to Special Delegates and Council members in Section 6.4.

### Section 12.9 -- Securities Law Disclaimer

(a) The Company makes no representation regarding the treatment of Governance Tokens under the securities laws of any jurisdiction.

(b) While the DAO Act provides that governance tokens conferring no economic rights are not deemed securities under Marshall Islands law, this Governance Token confers economic rights (including the right to distributions under Article VII). Accordingly, Governance Tokens may be deemed securities in one or more jurisdictions.

(c) The Marshall Islands exempts DAO LLCs from the Marshall Islands Securities and Investment Act to the extent that the Company is not issuing, selling, exchanging, or transferring any digital securities to residents of the Republic of the Marshall Islands.

(d) Each Member is solely responsible for ensuring that their acquisition, holding, and transfer of Governance Tokens complies with the securities laws of their jurisdiction of residence or citizenship.

(e) GOVERNANCE TOKENS HAVE NOT BEEN REGISTERED UNDER THE U.S. SECURITIES ACT OF 1933, AS AMENDED, OR UNDER THE SECURITIES LAWS OF ANY OTHER JURISDICTION. THE COMPANY MAKES NO REPRESENTATION THAT GOVERNANCE TOKENS ARE NOT SECURITIES UNDER THE LAWS OF ANY JURISDICTION.

---

## LEGAL DISCLAIMERS

**THIS OPERATING AGREEMENT IS A TEMPLATE AND DOES NOT CONSTITUTE LEGAL ADVICE.** This document is provided for informational and educational purposes only and should not be relied upon as legal, tax, financial, or other professional advice.

**LEGAL REVIEW REQUIRED.** Before using this template, you should consult with a qualified attorney licensed in the Republic of the Marshall Islands and/or your own jurisdiction to review and customize this Agreement for your specific circumstances.

**NO ATTORNEY-CLIENT RELATIONSHIP.** The provision of this template does not create an attorney-client or fiduciary relationship between the template provider and any user.

**REGULATORY COMPLIANCE.** The regulatory treatment of DAOs, blockchain-based entities, tokens, and digital assets varies by jurisdiction and is subject to change. Users are responsible for ensuring compliance with all applicable laws and regulations.

**TAX CONSIDERATIONS.** While this Agreement addresses the Marshall Islands tax treatment (3% GRT on earned revenue and interest), Members remain subject to the tax laws of their own jurisdictions. Members should consult with qualified tax advisors regarding their individual tax obligations.

**SMART CONTRACT RISKS.** Smart contracts deployed on the Solana blockchain are subject to technical risks including bugs, exploits, network outages, and validator failures. The inclusion of smart contract addresses in this Agreement does not guarantee the security or correct operation of such smart contracts.

**JURISDICTIONAL LIMITATIONS.** The enforceability of this Agreement and the legal protections afforded by the Marshall Islands DAO LLC structure may vary depending on the jurisdiction in which enforcement is sought.

---

## EXHIBITS

### Exhibit A: Initial Members

| Member Identifier | Solana Wallet Address | Governance Tokens | Membership Interest (%) |
|-------------------|----------------------|-------------------|------------------------|
| [MEMBER_1_NAME] | [MEMBER_1_WALLET] | [MEMBER_1_TOKENS] | [MEMBER_1_PERCENTAGE]% |
| [MEMBER_2_NAME] | [MEMBER_2_WALLET] | [MEMBER_2_TOKENS] | [MEMBER_2_PERCENTAGE]% |
| [MEMBER_3_NAME] | [MEMBER_3_WALLET] | [MEMBER_3_TOKENS] | [MEMBER_3_PERCENTAGE]% |
| [ADDITIONAL_MEMBERS] | | | |

### Exhibit B: Multisig Signers

**Squads Protocol v4 Multisig Address**: [MULTISIG_ADDRESS]

**Approval Threshold**: [MULTISIG_THRESHOLD] of [MULTISIG_TOTAL]

| Signer | Role | Solana Wallet Address |
|--------|------|---------------------|
| [SIGNER_1_NAME] | [SIGNER_1_ROLE] | [SIGNER_1_WALLET] |
| [SIGNER_2_NAME] | [SIGNER_2_ROLE] | [SIGNER_2_WALLET] |
| [SIGNER_3_NAME] | [SIGNER_3_ROLE] | [SIGNER_3_WALLET] |
| [ADDITIONAL_SIGNERS] | | |

### Exhibit C: Special Delegates

| Delegate | Delegated Powers | Spending Authority | Term |
|----------|-----------------|-------------------|------|
| [DELEGATE_1_NAME] | [DELEGATE_1_POWERS] | Up to [DELEGATE_1_LIMIT] per transaction | [DELEGATE_1_TERM] |
| [DELEGATE_2_NAME] | [DELEGATE_2_POWERS] | Up to [DELEGATE_2_LIMIT] per transaction | [DELEGATE_2_TERM] |
| [ADDITIONAL_DELEGATES] | | | |

### Exhibit D: Smart Contract Specifications

**Blockchain**: Solana

**Network**: Mainnet-Beta (Cluster URL: https://api.mainnet-beta.solana.com)

**Token Program**: Token-2022 (Program ID: `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`)

| Contract | Type | Address | Purpose |
|----------|------|---------|---------|
| Cap Table Program | Anchor Program | [CAP_TABLE_PROGRAM_ADDRESS] | Entity, share class, and member record management |
| Governance Program | Anchor Program | [PROGRAM_ADDRESS] | On-chain governance, proposals, and voting |
| Governance Token Mint | Token-2022 SPL Token | [TOKEN_MINT] | Membership Interest representation |
| Transfer Hook Program | Anchor Program | [TRANSFER_HOOK_ADDRESS] | Transfer restriction enforcement (KYC, sanctions, lock-up) |
| Squads Multisig | Squads Protocol v4 | [MULTISIG_ADDRESS] | Multi-party approval for treasury and administration |
| Entity PDA | Program Derived Address | [ENTITY_PDA_ADDRESS] | On-chain entity metadata and charter hash |
| [ADDITIONAL_CONTRACT_NAME] | [ADDITIONAL_CONTRACT_TYPE] | [ADDITIONAL_CONTRACT_ADDRESS] | [ADDITIONAL_CONTRACT_PURPOSE] |

**Token-2022 Extensions Enabled**:
- Transfer Hook (compliance enforcement via [TRANSFER_HOOK_ADDRESS])
- Permanent Delegate (legal compliance, forced transfers -- delegate: [MULTISIG_ADDRESS])
- Metadata (on-chain share class details and legal document URI)
- [CHOOSE: Default Account State (Frozen until KYC verified) / No Default Account State]
- [ADDITIONAL_EXTENSIONS]

**Governance Token Parameters**:
- Mint Address: [TOKEN_MINT]
- Decimals: [TOKEN_DECIMALS]
- Maximum Supply: [MAX_TOKEN_SUPPLY]
- Initial Supply: [INITIAL_TOKEN_SUPPLY]
- Mint Authority: [MULTISIG_ADDRESS] (Squads Multisig Vault PDA)
- Freeze Authority: [MULTISIG_ADDRESS] (Squads Multisig Vault PDA)

### Exhibit E: Governance Program Specifications

| Parameter | Value |
|-----------|-------|
| Minimum Tokens for Proposal | [MINIMUM_PROPOSAL_TOKENS] |
| Ordinary Proposal Quorum | [ORDINARY_QUORUM]% |
| Ordinary Proposal Approval | >50% |
| Ordinary Voting Period | [ORDINARY_VOTING_PERIOD] days |
| Amendment Quorum | [AMENDMENT_QUORUM]% |
| Amendment Approval | [AMENDMENT_APPROVAL]% |
| Amendment Voting Period | [AMENDMENT_VOTING_PERIOD] days |
| Dissolution Quorum | [DISSOLUTION_QUORUM]% |
| Dissolution Approval | [DISSOLUTION_APPROVAL]% |
| Dissolution Voting Period | [DISSOLUTION_VOTING_PERIOD] days |
| Multisig Threshold | [MULTISIG_THRESHOLD] of [MULTISIG_TOTAL] |
| Time Lock (Standard) | [TIMELOCK_PERIOD] hours |
| Time Lock (Emergency) | 0 hours |
| Time Lock (Smart Contract Upgrade) | [UPGRADE_TIMELOCK_PERIOD] hours |

---

**IN WITNESS WHEREOF**, the Members have executed this Operating Agreement as of the Effective Date first written above, by the act of holding Governance Tokens at mint address [TOKEN_MINT] on the Solana blockchain, which act constitutes agreement to and acceptance of all terms and conditions herein.

---

*This Operating Agreement was ratified by on-chain governance vote. Solana transaction signature: [RATIFICATION_TX_HASH]*

*Operating Agreement SHA-256 Hash: [AGREEMENT_HASH]*

*Hash anchored on-chain at Entity PDA: [ENTITY_PDA_ADDRESS]*
