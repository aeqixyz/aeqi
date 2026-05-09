# aeqi-indexer

Solana log indexer for the AEQI protocol. Replaces the EVM event indexer that previously watched the Base contracts.

## What it does

- WebSocket-subscribes to logs of all 7 AEQI programs via `logsSubscribe`
- Decodes Anchor events from `Program data:` lines (base64 of `8-byte discriminator || borsh payload`)
- Projects events into a SQLite DB matching the existing `aeqi-indexer` schema (so the runtime / UI doesn't notice the chain swap)

## Skeleton (this iteration)

- ✅ Connect to RPC + WS
- ✅ Subscribe to all 7 program log streams in parallel
- ✅ Decode Anchor `Program data:` lines
- 🟡 Print decoded events to stdout (DB writes pending)
- 🔴 `getSignaturesForAddress` backfill
- 🔴 Two-tier projection (finalized for trust mutations, confirmed for UI optimism)
- 🔴 Idempotent crash recovery keyed by `(program_id, slot, sig)`
- 🔴 Discriminator → typed event registry (per-program decoders)

## Run

```bash
cargo build --release
AEQI_INDEXER_WS=ws://127.0.0.1:9900 ./target/release/aeqi-indexer
```

For Solana mainnet:

```bash
AEQI_INDEXER_WS=wss://api.mainnet-beta.solana.com \
AEQI_INDEXER_COMMITMENT=finalized \
./target/release/aeqi-indexer
```

Production: **public RPC** (Helius / Triton / Solana Foundation public). Per `feedback_use_public_solana_rpc.md` — self-hosting an agave-validator RPC node is out-of-scope (~$500-1500/mo + weekly upgrade churn). If a paid tier of the SAME provider isn't enough, that's the trigger to re-litigate.

## Programs watched

| Program | ID |
|---|---|
| aeqi_trust | `AF9cqzwiGCf2XHtLXyKJwToPaJghmEaHa9VQJ1zjoUHs` |
| aeqi_factory | `7rX3fnJUy7tDSpo1EGCnUhs1XnxxbsQzXXNDCTh64v6n` |
| aeqi_role | `HFqh9bPLS7EwirMsz9MpNT96SN5v2JBeKTdnUpSVyuVe` |
| aeqi_governance | `528PTeSk8M3pKMMhc5vitbcwMGUMcHMzg6G5XpX8iVBn` |
| aeqi_token | `V9WiXaeayA8KTyVAEEG1rAuPQ28G6NEwzSCmzZNZv6z` |
| aeqi_treasury | `CQ7TGZFmkoZh61xgKnbjcj9Uomht38LqeihMNsY4p9KC` |
| aeqi_vesting | `24mJEeCHs492NGCJADvfb9zWDcqoDWNCpCYC2xAE2VBs` |
