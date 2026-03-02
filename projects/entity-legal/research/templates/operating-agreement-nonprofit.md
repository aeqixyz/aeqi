# OPERATING AGREEMENT OF [ENTITY NAME] DAO LLC

## A Non-Profit Series Decentralized Autonomous Organization Limited Liability Company Organized Under the Laws of the Republic of the Marshall Islands

**Effective Date**: [DATE]

**Registration Number**: [REGISTRATION NUMBER]

---

This Operating Agreement (this "Agreement") of [ENTITY NAME] DAO LLC (the "Company"), a non-profit decentralized autonomous organization limited liability company organized as a Series LLC under the laws of the Republic of the Marshall Islands, is entered into and effective as of [DATE] (the "Effective Date"), by and among the Members (as defined herein) of the Company.

WHEREAS, the Company was formed as a non-profit DAO LLC pursuant to the Non-Profit Entities (Amendment) Act of 2021, the Decentralized Autonomous Organization Act of 2022 (Public Law 2022-50), as amended by the 2023 Amendments and the 2024 Regulations (collectively, the "DAO Act"), and the Revised Uniform Limited Liability Company Act (Title 52 MIRC Chapter 4) (the "LLC Act") of the Republic of the Marshall Islands;

WHEREAS, the Company is structured as a Series LLC under the Series DAO LLC provisions introduced by the 2023 Amendments to the DAO Act, enabling the creation of one or more legally independent series within the Company, each with separate assets, liabilities, governance, and membership;

WHEREAS, the Company utilizes a two-token structure consisting of Security Tokens and Utility Tokens deployed on the Solana blockchain, with the smart contract serving as the authoritative membership registry as recognized by the DAO Act;

WHEREAS, as a non-profit DAO LLC, all digital assets including non-fungible tokens issued by the Company shall not be deemed a digital security pursuant to the DAO Act;

WHEREAS, [FOUNDATION NAME] (the "Foundation"), a non-profit DAO LLC organized under the laws of the Republic of the Marshall Islands, has been designated as the Special Delegate of the Company to perform limited administrative, compliance, and representative functions;

NOW, THEREFORE, in consideration of the mutual covenants and agreements set forth herein, and for other good and valuable consideration, the receipt and sufficiency of which are hereby acknowledged, the Members agree as follows:

---

## ARTICLE I: ORGANIZATION

### Section 1.1 — Name

The name of the Company is [ENTITY NAME] DAO LLC. The Company may conduct business under such name or any other name approved by the Members through the Governance Program.

### Section 1.2 — Entity Type and Designation

The Company is a non-profit Decentralized Autonomous Organization Limited Liability Company organized as a Series LLC. The Company is designated as a "[MEMBER-MANAGED / ALGORITHMICALLY MANAGED]" DAO LLC under the DAO Act. Non-profit designation means that no part of the income or profit of the Company is distributable to Members, directors, or officers, and the Company is prohibited from engaging in propaganda, legislative influence (with limited exceptions), or political campaign participation.

### Section 1.3 — Formation

The Company was formed on [FORMATION DATE] upon the filing of the Certificate of Formation with the Registrar of Corporations of the Republic of the Marshall Islands and the issuance of the Certificate of Formation by the Registrar.

### Section 1.4 — Governing Law

This Agreement and the rights, obligations, and liabilities of the Members shall be governed by and construed in accordance with the laws of the Republic of the Marshall Islands, including the Non-Profit Entities (Amendment) Act of 2021, the DAO Act, and the LLC Act. Where the foregoing statutes are silent, the Company follows Delaware LLC precedent as incorporated by reference in RMI law, except where conflicting RMI precedent or law exists.

### Section 1.5 — Legal Hierarchy

In the event of any conflict between the sources of authority governing the Company, the following hierarchy of legal application shall apply, with the highest-listed authority prevailing:

(a) The DAO Act, the Non-Profit Entities (Amendment) Act of 2021, and other applicable law of the Republic of the Marshall Islands;

(b) The LLC Act and other applicable conventional LLC law of the Republic of the Marshall Islands;

(c) This written Operating Agreement; and

(d) The Smart Contract Code (as defined in Article II).

### Section 1.6 — Registered Agent and Office

The registered agent of the Company is [REGISTERED AGENT NAME], with a registered office at [REGISTERED OFFICE ADDRESS], Republic of the Marshall Islands. The Company shall continuously maintain a registered agent and registered office within the Republic of the Marshall Islands as required by the LLC Act.

### Section 1.7 — Purpose

The Company is organized exclusively for non-profit purposes, including but not limited to [DESCRIBE PURPOSE: e.g., "the development and maintenance of open-source software protocols, the funding of public goods, the administration of community grants programs, the governance of decentralized infrastructure, and the coordination of AI agent collectives"]. The Company may engage in any and all activities necessary, convenient, desirable, or incidental to the foregoing, provided that such activities are consistent with the non-profit restrictions set forth in Article VIII.

### Section 1.8 — Duration

The Company shall have perpetual existence unless dissolved in accordance with Article XI of this Agreement.

### Section 1.9 — Foreign Investment Business License

The Company has obtained or shall obtain a Foreign Investment Business License ("FIBL") as required by the laws of the Republic of the Marshall Islands for entities engaging in business in the jurisdiction.

---

## ARTICLE II: SMART CONTRACT INTEGRATION

### Section 2.1 — Solana Program

The Company is managed in part through a smart contract program (the "Smart Contract" or "Program") deployed on the Solana blockchain at the following publicly available identifier, as required by the DAO Act:

**Program Address**: [SOLANA PROGRAM ADDRESS]

**Blockchain Network**: Solana Mainnet (Cluster: mainnet-beta)

This Program address constitutes the "publicly available identifier of any smart contract directly used to manage the DAO" as required by the DAO Act for inclusion in the certificate of formation and this Agreement.

### Section 2.2 — Security Token

The Company issues a security token (the "Security Token") representing membership interests in the Company, deployed as an SPL Token-2022 token on the Solana blockchain at the following mint address:

**Security Token Mint Address**: [SECURITY TOKEN MINT ADDRESS]

Pursuant to the DAO Act, "all digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security." The Security Token issued by the Company benefits from this absolute statutory safe harbor and is not a digital security under Marshall Islands law regardless of its characteristics.

The Security Token incorporates the following Token-2022 extensions:

(a) **Transfer Hook**: Enforces compliance checks on every transfer, including KYC verification when any single holder's balance would exceed the Informal Holder Threshold (as defined in Section 3.5), Restricted Person screening, and lock-up period enforcement;

(b) **Permanent Delegate**: Designates the Foundation's Squads multisig vault as the permanent delegate, enabling forced transfers exclusively for legal compliance purposes including court orders, sanctions enforcement, and regulatory requirements;

(c) **Metadata**: Points to legal documentation describing the rights, restrictions, and obligations associated with the Security Token;

(d) **Default Account State**: All token accounts are created in a frozen state and are unfrozen only upon satisfaction of applicable compliance requirements or confirmation that the holder's balance remains below applicable thresholds.

### Section 2.3 — Utility Token

The Company issues a utility token (the "Utility Token") conferring governance rights and no economic rights, deployed as an SPL Token-2022 token on the Solana blockchain at the following mint address:

**Utility Token Mint Address**: [UTILITY TOKEN MINT ADDRESS]

The Utility Token is not a security under Marshall Islands law. Pursuant to the 2023 Amendment to the DAO Act, "a governance token conferring no economic rights shall not be deemed a security." Additionally, as a token issued by a non-profit DAO LLC, the Utility Token benefits from the absolute statutory safe harbor providing that "all digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security." The Utility Token confers governance rights only, including the right to submit proposals, cast votes, and delegate voting power.

The Utility Token incorporates the following Token-2022 extensions:

(a) **Transfer Hook**: Validates that the transferee is an Active Member or will become an Active Member upon receipt of the token;

(b) **Metadata**: Points to governance documentation describing the governance rights associated with the Utility Token.

### Section 2.4 — Smart Contract as Authoritative Membership Registry

Pursuant to the DAO Act, which provides that "membership in the DAO LLC can be based on the token holding criterion and tracked on-chain," and that "members' ownership of such a company may be defined in such a plain document as the register of members AND in the company's smart contract," the Smart Contract serves as the authoritative membership registry of the Company. The on-chain Member Registry maintained by the Smart Contract satisfies the Company's obligation to maintain a register of members. No duplicate off-chain membership registry is required.

### Section 2.5 — Operating Agreement Hash

A SHA-256 hash of this Agreement is stored in the Entity PDA (Program Derived Address) maintained by the Smart Contract as the `charter_hash` field. The Entity PDA is derived using the seeds `[b"entity", entity_id.as_bytes()]` at the Program address specified in Section 2.1.

### Section 2.6 — Blockchain Records

Pursuant to the DAO Act, which provides that "written or paper records are not required if they are maintained on the blockchain," the Company may maintain any records required by the LLC Act or the DAO Act on the Solana blockchain in lieu of written or paper records. The blockchain record shall be the authoritative source for membership status, token balances, governance outcomes, and transaction history.

### Section 2.7 — Smart Contract Technical Summary

The Smart Contract implements the following autonomous operations: entity management, share class creation, member registration, token issuance, utility-to-security token swaps with KYC gating, permissionless security-to-utility token swaps, governance proposal creation, token-weighted vote casting, proposal execution, KYC status management, series creation, and authority rotation. A detailed technical description of the Smart Contract is attached hereto as Exhibit A.

---

## ARTICLE III: TWO-TOKEN STRUCTURE

### Section 3.1 — Security Token Rights

Holders of Security Tokens ("Security Token Holders") hold membership interests in the Company. The Security Token represents a non-economic membership interest. As a non-profit DAO LLC, the Company is prohibited from distributing profits to Members. Security Token Holders are entitled to:

(a) **Membership Interest**: A proportional membership interest in the Company corresponding to the Security Token Holder's share of total outstanding Security Tokens;

(b) **Residual Interest Upon Dissolution**: Upon dissolution and liquidation of the Company pursuant to Article XI, the right to participate in the distribution of remaining assets after satisfaction of all liabilities and obligations, to the extent consistent with the non-profit restrictions in Article VIII;

(c) **No Profit Distributions**: Security Tokens do not confer any right to distributions of profits, dividends, or profit shares during the ongoing operation of the Company.

Security Tokens do not confer governance rights. Security Token Holders who wish to participate in governance must also hold Utility Tokens or swap their Security Tokens for Utility Tokens pursuant to Section 3.3.

### Section 3.2 — Utility Token Rights

Holders of Utility Tokens ("Utility Token Holders") are entitled to the following governance rights:

(a) **Voting**: The right to cast votes on proposals submitted through the Governance Program, weighted in proportion to the number of Utility Tokens held (one vote per Utility Token);

(b) **Proposal Submission**: The right to submit proposals for consideration by the Members through the Governance Program;

(c) **Delegation**: The right to delegate voting power to another Member;

(d) **No Economic Rights**: The Utility Token confers no right to distributions, dividends, profit shares, liquidation preferences, or any other economic benefit.

### Section 3.3 — Token Swap Mechanism

Members may swap between Security Tokens and Utility Tokens through the Smart Contract, subject to the following conditions:

**(a) Utility Token to Security Token Swap (KYC Required)**:

(i) The Member initiates the swap through the Company's dashboard or directly through the Smart Contract;

(ii) The Foundation triggers the KYC verification process through its integrated KYC provider;

(iii) The Member completes KYC verification, providing a nonexpired passport, proof of residential address, and selfie verification;

(iv) The Foundation's KYC provider performs sanctions screening against FATF, UN Security Council, HMT, US, and EU sanctions lists;

(v) Upon successful verification, the Foundation writes a KYC hash (SHA-256 of verification data) to the Member's MemberRecord PDA on-chain;

(vi) The Foundation approves the swap transaction through its Squads multisig;

(vii) The Smart Contract burns the specified quantity of Utility Tokens from the Member's account and mints an equivalent quantity of Security Tokens to the Member's account;

(viii) The Transfer Hook validates the Member's KYC status before completing the mint operation.

**(b) Security Token to Utility Token Swap (No KYC Required)**:

(i) The Member initiates the swap through the Company's dashboard or directly through the Smart Contract;

(ii) The Smart Contract executes the swap directly, burning the specified quantity of Security Tokens and minting an equivalent quantity of Utility Tokens;

(iii) No Foundation approval or KYC verification is required for this direction of swap.

### Section 3.4 — Foundation as Swap Facilitator

The Foundation acts as the KYC escrow for all Utility Token to Security Token swaps. The Foundation holds the responsibility for verifying Member identity, performing sanctions screening, and writing KYC verification hashes to the blockchain. The Foundation maintains encrypted KYC records off-chain in compliance with applicable data protection requirements.

### Section 3.5 — Informal Holder Threshold

Pursuant to the 2024 DAO Regulations, a Beneficial Owner is defined as a person who exercises control through "more than 25% of the LLC's interests or voting rights" (the "Informal Holder Threshold"). The following provisions apply:

(a) Members holding Security Tokens or Utility Tokens representing twenty-five percent (25%) or less of the total outstanding supply of the applicable token class are not Beneficial Owners and are not required to complete KYC verification. Such Members may remain anonymous.

(b) Any Member whose Security Token or Utility Token holdings would exceed twenty-five percent (25%) of the total outstanding supply of the applicable token class must complete full KYC verification before the transfer or issuance that would cause the threshold to be exceeded can be executed. The Transfer Hook enforces this requirement programmatically.

(c) Members holding more than ten percent (10%) but not more than twenty-five percent (25%) of governance rights (Utility Tokens) are classified as "Significant Holders" and must complete KYC with the local regulator in accordance with the 2024 Regulations.

(d) All founders and incorporators of the Company must complete full KYC at the time of formation, regardless of their token holdings.

(e) At least one Ultimate Beneficial Owner ("UBO") must be identified and must undergo full KYC screening, regardless of token holdings or thresholds. The Foundation satisfies this requirement.

(f) KYC for Members exceeding the Informal Holder Threshold is renewed annually, typically in January, coinciding with the annual report filing period.

### Section 3.6 — Securities Treatment

All digital assets issued by the Company, including the Security Token, the Utility Token, and any non-fungible tokens, are not digital securities under Marshall Islands law. Pursuant to the DAO Act, "all digital assets including non-fungible tokens issued by a non-profit DAO LLC shall not be deemed a digital security." This is an absolute statutory safe harbor. No case-by-case analysis is required. This exemption applies regardless of the characteristics of the token, the manner of its issuance, or the rights it confers.

---

## ARTICLE IV: FOUNDATION AS REPRESENTATIVE

### Section 4.1 — Designation as Special Delegate

[FOUNDATION NAME], a non-profit DAO LLC organized under the laws of the Republic of the Marshall Islands with registration number [FOUNDATION REGISTRATION NUMBER] (the "Foundation"), is hereby designated as a Special Delegate of the Company pursuant to the DAO Act. The Foundation is not a director, officer, trustee, supervisor, or manager of the Company. The Foundation is a representative with limited, enumerated authority as defined in this Article IV.

### Section 4.2 — Scope of Authority

The Foundation's authority is limited to the following administrative, compliance, and representative functions:

(a) Filing compliance documents with the Registrar of Corporations of the Republic of the Marshall Islands, including annual reports, Beneficial Owner Information Reports, and amendments to the Certificate of Formation;

(b) Maintaining the registered agent relationship on behalf of the Company;

(c) Performing KYC verification for Members participating in Utility Token to Security Token swaps or crossing the Informal Holder Threshold;

(d) Acting as escrow for the Utility Token to Security Token swap process, including writing KYC verification hashes to the blockchain and approving swap transactions;

(e) Interfacing with banking institutions and financial service providers on behalf of the Company as authorized by governance vote;

(f) Executing governance-approved administrative actions that require off-chain execution or interaction with third parties;

(g) Receiving service of process on behalf of the Company;

(h) Maintaining encrypted KYC records off-chain in compliance with applicable data protection requirements;

(i) Operating the Transfer Hook compliance infrastructure, including sanctions screening and Restricted Person monitoring.

### Section 4.3 — Limitations on Authority

The Foundation shall not, without prior approval by governance vote of the Utility Token Holders:

(a) Make business decisions on behalf of the Company;

(b) Issue, mint, burn, or transfer Security Tokens or Utility Tokens outside the authorized swap mechanism and compliance operations described in this Agreement;

(c) Change governance rules, voting thresholds, quorum requirements, or proposal parameters;

(d) Access, transfer, or encumber treasury funds or assets of the Company;

(e) Enter into contracts, agreements, or obligations on behalf of the Company with a value exceeding [THRESHOLD AMOUNT];

(f) Amend this Agreement or the Certificate of Formation;

(g) Create or dissolve any Series of the Company;

(h) Expand the scope of its own authority beyond that enumerated in Section 4.2;

(i) Distribute any profits, income, or economic benefits to any Member, director, officer, or any other person, in accordance with the non-profit restrictions in Article VIII.

### Section 4.4 — Foundation Squads Multisig

The Foundation exercises its on-chain authority through a Squads Protocol v4 multisig wallet with the following configuration:

**Multisig Address**: [FOUNDATION SQUADS MULTISIG ADDRESS]

**Threshold**: [THRESHOLD]-of-[TOTAL] signers

**Time Lock**: [TIME LOCK DURATION] hours

The multisig controls three vaults:

(a) **Vault 0** (Foundation Operations): Funds allocated for the Foundation's operational expenses in performing its duties under this Agreement;

(b) **Vault 1** (Program Upgrade Authority): Authority to upgrade the Smart Contract, exercisable only upon governance approval;

(c) **Vault 2** (Entity Authority): Authority to sign administrative transactions on behalf of the Company, including KYC updates, compliance transfers, and authority rotations.

### Section 4.5 — Custodial Arrangement

The Foundation holds one hundred percent (100%) of the Security Tokens on behalf of the Security Token Holders as a custodial arrangement. The Foundation is the registered holder of all Security Tokens. Beneficial ownership of Security Tokens maps to individual token holders based on on-chain records maintained in MemberRecord PDAs and token balances in user PDA wallets. The Smart Contract enforces that the Foundation cannot transfer Security Tokens except through the authorized swap mechanism described in Section 3.3 and for compliance purposes through the Permanent Delegate extension described in Section 2.2(b).

### Section 4.6 — Removal of Foundation

The Foundation may be removed as Special Delegate by a governance vote of the Utility Token Holders requiring approval of not less than [SUPERMAJORITY THRESHOLD]% of the Utility Tokens cast, with a quorum of not less than [QUORUM]% of the total outstanding Utility Tokens. Upon removal, the Utility Token Holders shall designate a successor Special Delegate. The transition shall include transfer of all compliance records, KYC data, and administrative responsibilities to the successor.

### Section 4.7 — No Fiduciary Duty

The Foundation, in its capacity as Special Delegate, does not owe fiduciary duties to the Company or to any Member beyond the express obligations set forth in this Agreement and the implied covenant of good faith and fair dealing. This provision is consistent with the DAO Act, which permits waiver of fiduciary duties in the operating agreement.

---

## ARTICLE V: MEMBERSHIP

### Section 5.1 — Membership by Token Holding

Membership in the Company is determined by token holding, as authorized by the DAO Act which provides that "membership in the DAO LLC can be based on the token holding criterion and tracked on-chain." Any person or entity holding one or more Security Tokens or Utility Tokens is a Member of the Company. Token holders automatically become Members without requiring separate onboarding, registration, or written agreement, except to the extent that KYC is required pursuant to Section 3.5. By acquiring tokens, a Member accepts and agrees to be bound by this Agreement.

### Section 5.2 — Single Membership Class

The Company has a single class of membership. All Members, whether holding Security Tokens, Utility Tokens, or both, are Members of the same class. Membership interests and governance rights are differentiated by token type, not by membership class.

### Section 5.3 — Membership Transfer

Membership interests in the Company are transferred simultaneously with token transfers on-chain, as provided by the DAO Act which states that "membership transfers occur simultaneously with token transfers on-chain." No separate transfer agreement, consent of existing Members, or registration with the Company is required for a valid membership transfer, subject to the Transfer Hook's compliance checks.

### Section 5.4 — Automatic Dissociation

A Member is automatically dissociated from the Company when the Member's combined balance of Security Tokens and Utility Tokens reaches zero. Upon dissociation, the Member ceases to have any rights, obligations, or interests in the Company. Dissociation is effective immediately upon the on-chain record reflecting a zero combined token balance.

### Section 5.5 — Restricted Persons

A "Restricted Person" is any individual or entity that is: (a) listed on the sanctions lists maintained by FATF, the United Nations Security Council, HM Treasury, the United States Office of Foreign Assets Control, or the European Union; (b) a resident or national of any country subject to comprehensive sanctions by the foregoing authorities; or (c) determined by the Foundation to be in violation of the Company's adopted AML policies. Any Member determined to be a Restricted Person shall be automatically dissociated from the Company. The Transfer Hook rejects transfers involving Restricted Persons. The Foundation monitors sanctions lists on an ongoing basis and updates the `restricted_person` flag in the relevant MemberRecord PDA.

### Section 5.6 — Privacy

Members holding Security Tokens or Utility Tokens representing twenty-five percent (25%) or less of the total outstanding supply of the applicable token class may remain anonymous. The Company shall not require such Members to disclose their identity except as required by applicable law. The Company shall protect Member privacy to the maximum extent permitted by law.

### Section 5.7 — No Minimum Member Requirement

The Company has no minimum number of Members. The Company may be formed and operated with one or more Members.

---

## ARTICLE VI: GOVERNANCE

### Section 6.1 — Governance by Utility Token Holders

Governance of the Company is exercised by the Utility Token Holders through the Governance Program deployed as part of the Smart Contract. Governance rights are proportional to Utility Token holdings (one vote per Utility Token held).

### Section 6.2 — Governance PDA

The governance configuration of the Company is maintained in a Governance PDA on the Solana blockchain, derived using seeds `[b"governance", entity_pda.key().as_ref()]`. The Governance PDA stores the voting threshold, quorum requirement, and proposal duration parameters.

### Section 6.3 — Proposal Types and Approval Thresholds

The following proposal types and corresponding approval thresholds shall apply:

| Proposal Type | Approval Threshold | Quorum |
|---|---|---|
| Ordinary Resolutions (general operations) | [ORDINARY THRESHOLD]% of votes cast | [ORDINARY QUORUM]% of outstanding Utility Tokens |
| Special Resolutions (amendment to Agreement, Series creation/dissolution) | [SPECIAL THRESHOLD]% of votes cast | [SPECIAL QUORUM]% of outstanding Utility Tokens |
| Extraordinary Resolutions (dissolution, Foundation removal) | [EXTRAORDINARY THRESHOLD]% of votes cast | [EXTRAORDINARY QUORUM]% of outstanding Utility Tokens |
| Grant Disbursements | [GRANT THRESHOLD]% of votes cast | [GRANT QUORUM]% of outstanding Utility Tokens |
| Smart Contract Upgrades | [UPGRADE THRESHOLD]% of votes cast | [UPGRADE QUORUM]% of outstanding Utility Tokens |

### Section 6.4 — Proposal Process

(a) Any Utility Token Holder holding at least [MINIMUM PROPOSAL TOKENS] Utility Tokens may submit a proposal through the Governance Program.

(b) Each proposal has a voting period of [VOTING PERIOD] days, commencing upon on-chain submission.

(c) Members cast votes by invoking the `cast_vote` instruction on the Smart Contract, weighted by the number of Utility Tokens held at the time of the vote.

(d) Upon expiration of the voting period, if the approval threshold and quorum are met, the proposal is deemed approved and may be executed by invoking the `execute_proposal` instruction on the Smart Contract.

(e) Proposals that fail to meet the approval threshold or quorum by the end of the voting period are deemed rejected.

### Section 6.5 — Voting Delegation

Utility Token Holders may delegate their voting power to another Member by executing a delegation transaction through the Smart Contract. Delegation is revocable at any time. Delegated voting power does not transfer membership interests or Utility Token ownership.

### Section 6.6 — On-Chain Execution

Approved proposals are executed on-chain through the Smart Contract where technically feasible. For approved proposals requiring off-chain execution, the Foundation shall execute the approved action within [EXECUTION PERIOD] days of approval, subject to the Foundation's scope of authority as defined in Article IV.

### Section 6.7 — Operating Agreement Amendments

Amendments to this Agreement require approval as a Special Resolution. The SHA-256 hash of the amended Agreement shall be written to the Entity PDA's `charter_hash` field upon effectiveness of the amendment. The amended Agreement shall be filed with the Registrar of Corporations.

---

## ARTICLE VII: SERIES

### Section 7.1 — Series Creation

The Company may create one or more series (each, a "Series") within the Company through the Governance Program. Creation of a Series requires approval as a Special Resolution. Each Series is registered under the Company's Certificate of Formation and documented in an amendment to this Agreement filed with the Registrar.

### Section 7.2 — Series Independence

Each Series established under this Agreement shall have:

(a) **Separate Assets**: Its own treasury, tokens, property, and contractual rights, held separately from all other Series and from the Company;

(b) **Separate Liabilities**: Its own debts, obligations, and liabilities. Creditors of one Series shall not have recourse to the assets of any other Series or to the assets of the Company not allocated to such Series. The liabilities of the Company incurred, contracted for, or otherwise existing with respect to a particular Series shall be enforceable against the assets of such Series only;

(c) **Separate Governance**: Its own voting rules, quorum requirements, and management model, which may differ from those of the Company and other Series;

(d) **Separate Membership**: Its own member composition, which may differ from the membership of the Company and other Series.

### Section 7.3 — Series On-Chain Structure

Each Series is represented on the Solana blockchain by a Series PDA derived using seeds `[b"series", entity_pda.key().as_ref(), series_name.as_bytes()]`. Each Series may have its own Security Token Mint and Utility Token Mint, separate from those of the Company and other Series.

### Section 7.4 — Inter-Series Liability Shield

The debts, liabilities, obligations, and expenses incurred, contracted for, or otherwise existing with respect to a particular Series shall be enforceable against the assets of such Series only, and not against the assets of the Company generally or any other Series. This liability shield is consistent with the Series LLC provisions of the 2023 Amendments to the DAO Act and follows the Delaware Series LLC precedent codified in 6 Del. C. Section 18-215.

### Section 7.5 — Series Operating Supplements

Each Series shall be governed by a Series Operating Supplement appended to this Agreement, specifying the Series name, purpose, governance parameters, token mint addresses, initial members, and any provisions that differ from this Agreement. To the extent a Series Operating Supplement is silent on any matter, the terms of this Agreement shall govern. Each Series must comply with the non-profit restrictions set forth in Article VIII.

### Section 7.6 — Series Dissolution

A Series may be dissolved by a Special Resolution of the Utility Token Holders of the Company or, if the Series has its own governance mechanism, by an Extraordinary Resolution of the Series' Utility Token Holders, subject to satisfaction of all liabilities of the Series.

---

## ARTICLE VIII: NON-PROFIT RESTRICTIONS

### Section 8.1 — Prohibition on Profit Distribution

No part of the income or profit of the Company shall be distributable to Members, directors, officers, or any other person. The Company shall not declare or pay dividends, profit shares, or any other form of profit distribution to any Member or other person.

### Section 8.2 — Permitted Treasury Activities

Notwithstanding Section 8.1, the Company may:

(a) Hold, manage, and invest treasury assets, including digital assets, stablecoins, and traditional financial instruments;

(b) Make grants and disbursements in furtherance of the Company's non-profit purpose, as approved through the Governance Program;

(c) Pay reasonable compensation for services rendered to or on behalf of the Company;

(d) Reimburse expenses incurred by Members, Special Delegates, or service providers in connection with Company activities;

(e) Fund the operations of Series established under the Company.

### Section 8.3 — Prohibited Activities

The Company shall not:

(a) Distribute profits, income, or economic benefits to Members in their capacity as Members;

(b) Engage in propaganda or attempts to influence legislation, except to the extent that the Company may make available the results of nonpartisan analysis, study, or research;

(c) Participate in, or intervene in (including the publishing or distributing of statements), any political campaign on behalf of or in opposition to any candidate for public office;

(d) Engage in any activity inconsistent with the non-profit purpose set forth in Section 1.7.

### Section 8.4 — Reasonable Compensation Standard

Compensation paid by the Company for services rendered shall be reasonable in amount and commensurate with the nature and scope of the services provided. The Governance Program shall approve all compensation arrangements exceeding [THRESHOLD AMOUNT] in value.

---

## ARTICLE IX: LIABILITY AND FIDUCIARY DUTIES

### Section 9.1 — Limited Liability

No Member shall be liable as such for the liabilities of the Company or any Series, regardless of any failure to observe formalities. The debts, obligations, and liabilities of the Company or any Series, whether arising in contract, tort, or otherwise, shall be solely the debts, obligations, and liabilities of the Company or such Series. A Member's liability to the Company is limited to the value of the Member's capital contribution, if any.

### Section 9.2 — Waiver of Fiduciary Duties

To the maximum extent permitted by the DAO Act and the LLC Act, this Agreement is not intended to, and does not, create or impose any fiduciary duty on any Member, Special Delegate, or other person. Each Member hereby waives, to the fullest extent permitted by law, any and all fiduciary duties that, absent this waiver, may be implied by law or equity. The Members' duties and obligations are limited to those expressly set forth in this Agreement and the implied covenant of good faith and fair dealing.

### Section 9.3 — Implied Covenant of Good Faith and Fair Dealing

Notwithstanding Section 9.2, each Member and the Foundation are subject to the implied covenant of good faith and fair dealing, which cannot be waived under the DAO Act.

### Section 9.4 — Open-Source Software Immunity

Pursuant to the 2023 Amendments to the DAO Act, the Company and its Members shall not be liable for any open-source software created, published, or contributed to by the Company, even if third parties misuse such software.

### Section 9.5 — Indemnification

The Company shall indemnify and hold harmless each Member, the Foundation, and any Special Delegate from and against any claims, losses, damages, liabilities, costs, and expenses (including reasonable attorneys' fees) arising out of or relating to the Member's, Foundation's, or Special Delegate's participation in the Company, except to the extent that such claims arise from the indemnified party's willful misconduct, gross negligence, or breach of this Agreement.

### Section 9.6 — Corporate Veil

The Company maintains a corporate veil separating the Company from its Members, consistent with the LLC Act and Delaware precedent. The corporate veil shall not be pierced except upon a finding by a court of competent jurisdiction of commingling of funds, inadequate capitalization, fraud, or alter ego status, in accordance with applicable law.

---

## ARTICLE X: COMPLIANCE

### Section 10.1 — Annual Report

The Company shall file an annual report with the Registrar of Corporations between January 1st and March 31st of each year. The annual report shall contain beneficial ownership information, leadership and management details, community engagement summary, confirmation of operational status, and any structural changes during the preceding year. The Foundation is responsible for preparing and filing the annual report.

### Section 10.2 — Beneficial Owner Information Report

The Company shall maintain and file a Beneficial Owner Information Report ("BOIR") identifying each Beneficial Owner. A Beneficial Owner is a person who exercises control through more than twenty-five percent (25%) of the LLC's interests or voting rights. The BOIR must include the Beneficial Owner's full legal name, date of birth, residential address, nonexpired passport number, and all wallet addresses associated with the Company. The BOIR shall be updated promptly upon any change in Beneficial Owner information.

### Section 10.3 — KYC Obligations

(a) All founders and incorporators shall complete full KYC at the time of formation.

(b) Members holding more than twenty-five percent (25%) of the Company's interests or voting rights shall complete full KYC, with annual renewal in January.

(c) Members holding more than ten percent (10%) of governance rights (Utility Tokens) shall complete KYC with the local regulator in accordance with the 2024 Regulations.

(d) The Foundation shall monitor the Informal Holder Threshold in real-time through the Transfer Hook and trigger KYC requirements automatically when a Member's holdings approach or exceed the threshold.

### Section 10.4 — AML Policy

The Company shall adopt and comply with an Anti-Money Laundering ("AML") policy consistent with international standards. The AML policy shall address: (a) monitoring of virtual asset transfers exceeding USD 1,000; (b) cross-border transaction monitoring; (c) sanctions screening; (d) Restricted Person identification and dissociation; and (e) record-keeping requirements. The Foundation is responsible for implementing and maintaining the AML policy.

### Section 10.5 — Smart Contract Updates

Material updates to the Smart Contract shall be disclosed to the Registrar of Corporations. Any update that changes the governance mechanism, membership criteria, token economics, or compliance enforcement of the Smart Contract shall require prior approval as a Smart Contract Upgrade proposal through the Governance Program and shall be reported in the next annual report.

### Section 10.6 — Corporate Continuation

The Company shall complete corporate continuation annually, including annual report filing, government fee payment, and registered agent fee payment. Failure to comply with corporate continuation requirements may result in penalties of USD 500 per day, fines of up to USD 10,000, and potential cancellation of the Certificate of Formation pursuant to Section 15 of the 2024 Regulations.

---

## ARTICLE XI: DISSOLUTION

### Section 11.1 — Dissolution Events

The Company shall be dissolved upon the occurrence of any of the following events:

(a) Approval of an Extraordinary Resolution by the Utility Token Holders specifically authorizing dissolution;

(b) Entry of a decree of judicial dissolution by a court of competent jurisdiction;

(c) Cancellation of the Certificate of Formation by the Registrar of Corporations for persistent non-compliance;

(d) Any other event that makes it unlawful for the Company to continue its business under the laws of the Republic of the Marshall Islands.

### Section 11.2 — Winding Up

Upon dissolution, the Foundation shall wind up the affairs of the Company, which shall include:

(a) Collecting all assets of the Company and each Series;

(b) Paying or making adequate provision for all debts, liabilities, and obligations of the Company and each Series;

(c) Distributing the remaining assets of the Company and each Series in a manner consistent with the non-profit purpose of the Company. Remaining assets shall be distributed to one or more organizations organized and operated exclusively for non-profit, charitable, educational, scientific, or public purposes, as determined by a Special Resolution of the Utility Token Holders. In no event shall remaining assets be distributed to Members in proportion to their membership interests.

(d) Filing a notice of dissolution with the Registrar of Corporations.

### Section 11.3 — Series Dissolution

Dissolution of a Series does not cause dissolution of the Company or any other Series. Assets of a dissolved Series shall be distributed in a manner consistent with the non-profit purpose of the Company and the Series, after satisfaction of all liabilities of the Series.

---

## ARTICLE XII: DISPUTE RESOLUTION

### Section 12.1 — Mandatory Negotiation

Any dispute, controversy, or claim arising out of or relating to this Agreement, or the breach, termination, or invalidity thereof, shall first be submitted to mandatory negotiation between the parties in good faith for a period of thirty (30) days from the date written notice of the dispute is delivered to the other party.

### Section 12.2 — Arbitration

If the dispute is not resolved through negotiation within the thirty (30) day period, the dispute shall be finally settled by binding arbitration administered by the International Centre for Dispute Resolution ("ICDR") in accordance with its International Arbitration Rules then in effect. The seat of arbitration shall be the Republic of the Marshall Islands. The language of the arbitration shall be English. The arbitral tribunal shall consist of [ONE / THREE] arbitrator(s). The award rendered by the arbitral tribunal shall be final, binding, and enforceable in any court of competent jurisdiction.

### Section 12.3 — Governing Law

This Agreement and all disputes arising out of or in connection with this Agreement shall be governed by and construed in accordance with the laws of the Republic of the Marshall Islands, without regard to conflict of laws principles.

### Section 12.4 — No Court Jurisdiction

To the maximum extent permitted by applicable law, the Members agree that no court shall have jurisdiction over any claim arising out of or relating to this Agreement, except for the enforcement of an arbitral award rendered pursuant to Section 12.2 and except as required by mandatory provisions of the DAO Act or the LLC Act.

### Section 12.5 — Costs

Each party shall bear its own costs and expenses in connection with any dispute resolution proceedings under this Article XII, unless the arbitral tribunal determines that a different allocation is warranted.

---

## ARTICLE XIII: GENERAL PROVISIONS

### Section 13.1 — Entire Agreement

This Agreement, together with the Certificate of Formation, the Exhibits hereto, and any Series Operating Supplements, constitutes the entire agreement among the Members with respect to the subject matter hereof and supersedes all prior agreements and understandings, whether written or oral.

### Section 13.2 — Severability

If any provision of this Agreement is held to be invalid, illegal, or unenforceable, the validity, legality, and enforceability of the remaining provisions shall not be affected or impaired thereby. The invalid, illegal, or unenforceable provision shall be modified to the minimum extent necessary to make it valid, legal, and enforceable while preserving its original intent.

### Section 13.3 — Electronic Execution

This Agreement may be executed electronically, including by wallet signature, digital signature, or other electronic means. Electronic execution shall be deemed equivalent to manual execution for all purposes. By acquiring Security Tokens or Utility Tokens, a Member is deemed to have executed and agreed to this Agreement.

### Section 13.4 — Notices

Notices under this Agreement may be delivered through the Company's governance mechanism, through on-chain messaging, or through such other means as the Company may designate from time to time. Notices to the Foundation shall be delivered to [FOUNDATION CONTACT].

### Section 13.5 — No Waiver

The failure of any Member or the Foundation to enforce any provision of this Agreement shall not be construed as a waiver of such provision or the right to enforce it at a later time.

### Section 13.6 — Counterparts

This Agreement may be executed in any number of counterparts, each of which shall be deemed an original and all of which together shall constitute one and the same agreement.

---

## EXHIBITS

### Exhibit A: Smart Contract Technical Summary

**Program Address**: [SOLANA PROGRAM ADDRESS]

**Blockchain**: Solana Mainnet (mainnet-beta)

**Framework**: Anchor

**Token Standard**: SPL Token-2022

**Instructions**:

| Instruction | Description |
|---|---|
| `initialize_entity()` | Creates the Entity PDA with entity metadata, authority pointer, and charter hash |
| `create_share_class()` | Deploys a Token-2022 mint with specified extensions for a share class |
| `add_member()` | Creates a MemberRecord PDA linking a wallet to KYC status and membership data |
| `issue_shares()` | Mints tokens of a specified share class to a member's token account |
| `swap_utility_to_security()` | Burns Utility Tokens and mints Security Tokens, gated by KYC verification |
| `swap_security_to_utility()` | Burns Security Tokens and mints Utility Tokens, permissionless |
| `create_proposal()` | Creates a governance proposal PDA with description and voting parameters |
| `cast_vote()` | Records a token-weighted vote on a proposal |
| `execute_proposal()` | Executes an approved proposal's on-chain actions |
| `update_member_kyc()` | Writes KYC hash and verification status to a MemberRecord PDA |
| `create_series()` | Creates a Series PDA within the entity |
| `rotate_authority()` | Changes the entity's authority to a new multisig or address |

**Transfer Hook Logic**: Every Token-2022 transfer invokes the Transfer Hook Program, which validates: (1) both parties are Active Members; (2) neither party is a Restricted Person; (3) for Security Tokens, the receiver's post-transfer balance does not exceed the Informal Holder Threshold without KYC; (4) KYC has not expired for holders above the threshold; (5) lock-up periods are respected.

### Exhibit B: Token Specifications

**Security Token**:

| Parameter | Value |
|---|---|
| Mint Address | [SECURITY TOKEN MINT ADDRESS] |
| Token Standard | SPL Token-2022 |
| Decimals | [DECIMALS] |
| Initial Supply | [INITIAL SUPPLY] |
| Extensions | Transfer Hook, Permanent Delegate, Metadata, Default Account State (Frozen) |
| Permanent Delegate | [FOUNDATION SQUADS VAULT 2 ADDRESS] |
| Transfer Hook Program | [TRANSFER HOOK PROGRAM ADDRESS] |
| Securities Status | Not a digital security (non-profit DAO LLC automatic exemption) |

**Utility Token**:

| Parameter | Value |
|---|---|
| Mint Address | [UTILITY TOKEN MINT ADDRESS] |
| Token Standard | SPL Token-2022 |
| Decimals | [DECIMALS] |
| Initial Supply | [INITIAL SUPPLY] |
| Extensions | Transfer Hook, Metadata |
| Transfer Hook Program | [TRANSFER HOOK PROGRAM ADDRESS] |
| Securities Status | Not a digital security (governance token exemption + non-profit automatic exemption) |

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

### Exhibit D: Governance Parameters

| Parameter | Value |
|---|---|
| Ordinary Resolution Threshold | [ORDINARY THRESHOLD]% |
| Ordinary Resolution Quorum | [ORDINARY QUORUM]% |
| Special Resolution Threshold | [SPECIAL THRESHOLD]% |
| Special Resolution Quorum | [SPECIAL QUORUM]% |
| Extraordinary Resolution Threshold | [EXTRAORDINARY THRESHOLD]% |
| Extraordinary Resolution Quorum | [EXTRAORDINARY QUORUM]% |
| Grant Disbursement Threshold | [GRANT THRESHOLD]% |
| Grant Disbursement Quorum | [GRANT QUORUM]% |
| Upgrade Threshold | [UPGRADE THRESHOLD]% |
| Upgrade Quorum | [UPGRADE QUORUM]% |
| Minimum Tokens for Proposal | [MINIMUM PROPOSAL TOKENS] |
| Voting Period | [VOTING PERIOD] days |

### Exhibit E: Initial Members

| Member | Wallet Address | Token Type | Amount | KYC Status |
|---|---|---|---|---|
| [FOUNDATION NAME] | [FOUNDATION WALLET] | Security Token | [AMOUNT] | Verified (UBO) |
| [MEMBER NAME / ANONYMOUS] | [WALLET ADDRESS] | [TOKEN TYPE] | [AMOUNT] | [STATUS] |

---

**IN WITNESS WHEREOF**, the Members have executed this Operating Agreement as of the Effective Date by acquiring Security Tokens or Utility Tokens of the Company, which acquisition constitutes acceptance of and agreement to this Operating Agreement.

[ENTITY NAME] DAO LLC
Republic of the Marshall Islands
Registration Number: [REGISTRATION NUMBER]
Effective Date: [DATE]
