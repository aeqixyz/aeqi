# AEQI Solana Hackathon MVP Cut

Date: 2026-05-09

## Goal

Ship a demo that proves AEQI can create an auditable Solana-native company workspace with on-chain company identity, roles, governance, and a simple cap table. The demo should be understandable in minutes and should not require judges or first users to reason about every experimental capital-formation module.

## MVP Surface

Core, demo-critical:

- `aeqi_trust`: company identity, module registry, config authority.
- `aeqi_factory`: one-click company spawn.
- `aeqi_role`: founder/director/operator roles and authority walk.
- `aeqi_token`: Token-2022 cap-table mint with max supply.
- `aeqi_governance`: proposal, vote, execute lifecycle.

Capital-formation proof point:

- Use one narrow path only: a founder forms a company, mints capped participation tokens, then runs one funding or commitment-sale demonstration.
- Do not pitch `aeqi_fund`, `aeqi_funding`, `aeqi_budget`, `aeqi_treasury`, `aeqi_vesting`, and `aeqi_unifutures` as production-ready all at once. Treat them as protocol roadmap unless each is separately threat-modeled and demo-tested.

## Security Position

The current MVP-safe rule is conservative: after creation, trust config, pause, and ACL writes require the trust authority. Live module-driven trust mutation is intentionally closed until a complete module signer model exists and has regression tests.

The regression test `aeqi_trust rejects post-finalize config writes from non-authority even with a high-ACL module account` protects this boundary.

## Demo Script

1. Create a company with `aeqi_factory.create_company_full`.
2. Show the trust PDA, module PDAs, and finalized module state.
3. Register role types: founder/director/operator.
4. Assign one role and show the vote checkpoint.
5. Create the cap-table mint with a hard max supply.
6. Mint a small allocation to a participant.
7. Create a governance proposal and pass it with either role or token voting.
8. Optional: show one capital formation primitive as a non-custodial extension, not as the core product.

## Verification

Required before any demo recording or submission:

```bash
anchor build
anchor test --skip-build
```

Current baseline after the trust hardening patch:

- `anchor build`: passes.
- `anchor test --skip-build`: 81 passing.

## Product Story

AEQI is not "a DAO kit." The MVP claim is narrower and stronger:

> AEQI turns a new founder workspace into an on-chain company primitive: roles, cap table, decisions, and future capital formation are born from one auditable Solana deployment path.

That is the USP. Keep the demo centered on that path.
