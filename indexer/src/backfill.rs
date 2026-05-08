//! Historical replay via `getSignaturesForAddress` + `getTransaction`.
//!
//! On startup, before the live tail kicks in, walk the chain backwards from
//! the newest signature for each program ID until we hit `cursor.last_slot`
//! (or the chain start). Decode events and insert via `Sink` — the
//! `UNIQUE(signature, program, event_type)` constraint makes the operation
//! idempotent so reruns + overlap with live tail are safe.

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

use crate::registry;
use crate::sink::Sink;

const PAGE_LIMIT: usize = 1000;

pub async fn backfill_program(
    rpc: &RpcClient,
    program_id: &Pubkey,
    program_name: &str,
    sink: Arc<Sink>,
) -> Result<usize> {
    let until_slot = sink.cursor(program_name)?.unwrap_or(0);
    let mut before: Option<Signature> = None;
    let mut total_inserted = 0usize;
    let mut total_seen = 0usize;
    let mut last_slot = 0u64;

    info!(program = %program_name, until_slot, "backfill starting");

    loop {
        let cfg = GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(PAGE_LIMIT),
            commitment: Some(CommitmentConfig::finalized()),
        };
        let page = rpc
            .get_signatures_for_address_with_config(program_id, cfg)
            .await?;
        if page.is_empty() {
            break;
        }
        for sig_info in &page {
            total_seen += 1;
            if sig_info.slot < until_slot {
                info!(
                    program = %program_name,
                    inserted = total_inserted,
                    seen = total_seen,
                    "backfill reached cursor — done"
                );
                return Ok(total_inserted);
            }
            if sig_info.err.is_some() {
                continue;
            }
            let sig = Signature::from_str(&sig_info.signature)?;
            let tx = match rpc
                .get_transaction(&sig, solana_transaction_status::UiTransactionEncoding::Json)
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    warn!(?e, sig = %sig_info.signature, "get_transaction failed; skipping");
                    continue;
                }
            };

            let logs = tx
                .transaction
                .meta
                .as_ref()
                .and_then(|m| Option::<Vec<String>>::from(m.log_messages.clone()));
            let Some(logs) = logs else {
                continue;
            };

            for line in &logs {
                if let Some(rest) = line.strip_prefix("Program data: ") {
                    if let Ok(bytes) = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        rest,
                    ) {
                        if bytes.len() >= 8 {
                            if let Some(meta) = registry::lookup(program_id, &bytes[..8]) {
                                match sink.record_event(
                                    meta.program,
                                    meta.event,
                                    sig_info.slot,
                                    &sig_info.signature,
                                    rest,
                                ) {
                                    Ok(true) => total_inserted += 1,
                                    Ok(false) => {} // dedup
                                    Err(e) => warn!(?e, "sink.record_event failed during backfill"),
                                }
                            }
                        }
                    }
                }
            }
            last_slot = sig_info.slot;
        }
        // Continue paginating from the last signature in this page.
        before = Some(Signature::from_str(&page.last().unwrap().signature)?);
    }

    if last_slot > 0 {
        sink.bump_cursor(program_name, last_slot)?;
    }
    info!(
        program = %program_name,
        inserted = total_inserted,
        seen = total_seen,
        "backfill finished (no more signatures)"
    );
    Ok(total_inserted)
}
