# AEIQ template config

Defines the data needed to spawn the AEIQ company via `aeqi_factory.instantiate_template`. Once the factory program is fully implemented (WS-S6), this is the JSON the Solana provisioner feeds in.

## Identity

| Field | Value |
|---|---|
| Template ID | `keccak256("aeiq.v1")` (32 bytes) |
| TRUST ID | freshly random 32 bytes per spawn |
| Authority | platform-managed signer (transitions to TRUST itself after finalize) |
| IPFS CID | operating agreement pinned via `aeqi-ipfs` |

## Modules to register

The factory issues a `register_module` per slot, then `init` then `finalize`:

| module_id (keccak256 of name) | program_id | trust_acl |
|---|---|---|
| `role` | `aeqi_role` | SetNumericConfig + SetBytesConfig + Execute |
| `token` | `aeqi_token` | TransferFunds + SetBytesConfig |
| `governance` | `aeqi_governance` | Execute + SetAclBetweenModules |

Future modules (lands once their program ships):
- `treasury`, `budget`, `vesting`, `funding`, `unifutures`

## Role types (per Role.module)

11 roles total — 1 director tier (board) + 10 operational (org chart):

| role_type_id (keccak256) | hierarchy | notes |
|---|---|---|
| `director` | 0 | board seats, on-chain signers |
| `ceo` | 1 | operational top |
| `cto` | 2 | technology |
| `coo` | 2 | operations |
| `cfo` | 2 | finance |
| `head_eng` | 3 | engineering manager |
| `head_design` | 3 | design lead |
| `eng` | 4 | individual contributor |
| `designer` | 4 | individual contributor |
| `ops` | 4 | individual contributor |
| `ea` | 4 | executive assistant — 18-tool deny + Telegram mention-gate (per memory) |

Hierarchy lower = higher authority. The role parent DAG is supplied as `parent_role_id` per role:

```
director (root)
  └── ceo
        ├── cto
        │     ├── head_eng → eng (×N)
        │     └── head_design → designer (×N)
        ├── coo → ops (×N)
        ├── cfo
        └── ea (cross-cutting; reports to ceo, mention-gated)
```

## Governance config

Two configs registered in parallel — proposers pick which at proposal time:

| governance_config_id | mode | quorum | support | voting_period |
|---|---|---|---|---|
| `[0u8; 32]` (token) | token-weighted | 40% | 50% | 5 days |
| `keccak256("director")` (role-multisig) | per-role multisig | 60% | 50% | 3 days |

`director` multisig threshold = 60% of director-role-count. The token vote pulls from `aeqi_token` via CPI; the multisig pulls from `aeqi_role::get_past_role_votes` via CPI.

## Token config (Token-2022 mint)

| Field | Value |
|---|---|
| name | `AEIQ` |
| symbol | `AEIQ` |
| decimals | 9 |
| max_supply | 100_000_000 * 10^9 |
| transfer hooks | none initially (compliance hook reserved for vesting integration) |

Initial allocations (sum to 100_000_000):
- 30% to treasury PDA (locked)
- 25% to founders (vested 4y, 1y cliff via `aeqi_vesting`)
- 20% to ESOP role (`eng` + `designer` + `ops` types)
- 15% to investors (allocated post-funding round)
- 10% to community/airdrop (locked until governance proposal)

## Initial agents

7 agents bound to roles — each gets an agent PDA on `aeqi_role`:

| Agent | Role | Notes |
|---|---|---|
| `architect` | `cto` | meta-agent, /studio chat-shape entrypoint |
| `pm` | `ceo` | product management |
| `eng-1` | `eng` (under head_eng) | autonomous engineering |
| `eng-2` | `eng` (under head_eng) | autonomous engineering |
| `designer` | `designer` (under head_design) | autonomous design |
| `ops-1` | `ops` (under coo) | operations |
| `ea` | `ea` | executive assistant — Telegram mention-gated, 18-tool deny |

## Treasury seed

Once `aeqi_treasury` ships:
- 1000 USDC SPL airdropped to AEIQ treasury PDA on devnet
- 0.1 SOL for rent + tx fees

## Reference

- EVM AEIQ memory: `project_aeiq_dogfood_company.md` (entity_id=59bc9fd3, original Base address `0x4a922...4edc`, being purged)
- EA pattern: `architecture_executive_assistant_pattern.md`
- Role primitive: `architecture_role_primitive.md`
- Board vs org chart: `architecture_board_vs_org_chart.md` (directors are NOT in operational chain)
