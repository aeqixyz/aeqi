# aeqi-solana

The on-chain protocol for AEQI on Solana. Replaces the EVM-Base stack at `aeqi-core/` (Solidity).

> **Decision: 2026-05-07.** Full rewrite, EVM-Base canonical → Solana canonical. Single coherent chain, no L1/L2 fragmentation tax, native fee payer, native session keys, secp256r1 passkey precompile. AEQI becomes the standard primitive on Solana — not a thin layer over Squads/Realms.

## Architecture

Modular DAO framework, ported faithfully from the Solidity design at `~/projects/aeqi-core/contracts/`.

### Programs

| Program | Solidity origin | Purpose |
|---|---|---|
| `aeqi_trust` | `core/TRUST.sol` | Module registry, ACL flags, config store, execute() gateway |
| `aeqi_factory` | `core/Factory.sol` | Template registry + instantiate flow with multi-sig approval gate |
| `aeqi_role` | `modules/Role.module.sol` | Role DAG (parent walk), role types, delegations, vote checkpoints |
| `aeqi_governance` | `modules/Governance.module.sol` | Proposals, voting — token-weighted + per-role-multisig modes |
| `aeqi_token` | `modules/Token.module.sol` | Cap-table SPL Token-2022 mint authority + allocations |
| `aeqi_unifutures` | `modules/Unifutures.module.sol` | Bonding curve / commitment sale / exit primitives |
| `aeqi_fund` | `modules/Fund.module.sol` | NAV-based fund accounting, LP shares, carry |
| `aeqi_funding` | `modules/Funding.module.sol` | Capital raise orchestration via Unifutures |
| `aeqi_budget` | `modules/Budget.module.sol` | Hierarchical treasury allocations per role |
| `aeqi_vesting` | `modules/Vesting.module.sol` | Cliff + duration vesting, FDV milestone unlock |

### Key translation calls

**Beacon proxy → program upgrade authority.** EVM beacons exist because EVM has no native upgrade. Solana programs are upgradable directly via `BPFLoaderUpgradeable`. The "multi-source delegation" semantics (per-TRUST module-implementation overrides) become per-PDA `module_program_id` config — each TRUST PDA stores the program ID it uses for each module slot. Different TRUSTs can point at different program-ID versions of the same module.

**SlotArrays versioning → PDA isolation.** EVM SlotArrays use `keccak256(BASE, timestamp)` to derive fresh storage slots per-instance. Solana PDAs are already namespaced per (program, seeds) — the per-instance isolation is automatic. The "swap-with-last" O(1) array semantics port to a `Vec<Pubkey>` field with manual bookkeeping, OR (preferred) we use indexed PDAs (`[b"role_member", trust, idx]`) with a separate count.

**ABI-encoded config → Anchor account data.** EVM modules `abi.decode(getBytesConfig(KEY), (T1, T2, T3))` in `finalizeModule()`. On Solana, the Factory passes a `Vec<u8>` of borsh-serialized config; modules `try_from_slice` it. Same shape — the encoding format swaps from ABI → Borsh.

**Bit-flag ACLs → unchanged.** EVM `uint256 trustAcl` with `(acl >> flag) & 1` → Solana `u64 trust_acl` with the same bitwise check. Direct port.

**Role parent-walk DAG → bounded CPI traversal.** EVM uses inline assembly to walk parent role IDs. Solana walks PDAs with a hard depth cap (e.g. 8) per `assert_authority` ix; off-chain client provides the walk path as `remaining_accounts` so each parent role PDA is loaded exactly once.

**Governance multiplexing → unchanged shape.** Proposal stores `governance_config_id`. Vote-power retrieval CPIs into `aeqi_token` (token mode) or `aeqi_role` (role-multisig mode) at `cast_vote` time using the proposal's snapshot `vote_start_slot`.

**Two-phase init → unchanged shape.** Factory sequence: (1) deploy TRUST PDA, (2) for each module: create module-program PDA bound to TRUST + call `init`, (3) wire ACLs in TRUST, (4) call `finalize` on every module → it loads its config and validates. Module init guard via `initialized: u8` field on module PDA.

### Module init/finalize ABI

Every module program exposes:

```rust
pub fn init(ctx: Context<InitModule>, trust: Pubkey) -> Result<()>;
pub fn finalize(ctx: Context<FinalizeModule>, config: Vec<u8>) -> Result<()>;
```

`init` is called by the factory immediately after the module PDA is created; it stores the parent TRUST and sets `initialized=1`. `finalize` is called after all ACLs are wired; it borsh-deserializes the module-specific config struct and validates. Modules borsh-decode unconditionally — config absent or malformed = revert. Same gotcha as EVM.

### Indexer

`aeqi-indexer` is rewritten for Solana — `programSubscribe` live tail per program ID, `getSignaturesForAddress` backfill, finalized-commitment projection for trust mutations + confirmed-tier for UI optimism. Self-hosted RPC by Phase 5.

## Status

- 2026-05-07: Repo scaffolded. `aeqi_trust` skeleton landing.

## Audit

Three Solana-native firms targeted: OtterSec / Neodyme / Sec3. Budget $25-35k. RFP in WS-S5.
