//! aeqi-indexer — Solana indexer for the AEQI protocol.
//!
//! Subscribes to logs of all 7 AEQI programs via `logsSubscribe` (WS) and
//! decodes Anchor events from the `Program data:` lines via a pre-computed
//! discriminator registry. Projects events into a sink (stdout for now;
//! SQLite next iteration).
//!
//! Architecture: hits a public Solana RPC (Helius / Triton / Solana
//! Foundation), per `feedback_use_public_solana_rpc.md` — we run the
//! indexer service ourselves but don't run a validator/RPC node.

mod registry;
mod sink;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::{info, warn};

const PROGRAMS: &[(&str, &str)] = &[
    ("aeqi_trust", "AF9cqzwiGCf2XHtLXyKJwToPaJghmEaHa9VQJ1zjoUHs"),
    ("aeqi_factory", "7rX3fnJUy7tDSpo1EGCnUhs1XnxxbsQzXXNDCTh64v6n"),
    ("aeqi_role", "HFqh9bPLS7EwirMsz9MpNT96SN5v2JBeKTdnUpSVyuVe"),
    ("aeqi_governance", "528PTeSk8M3pKMMhc5vitbcwMGUMcHMzg6G5XpX8iVBn"),
    ("aeqi_token", "V9WiXaeayA8KTyVAEEG1rAuPQ28G6NEwzSCmzZNZv6z"),
    ("aeqi_treasury", "CQ7TGZFmkoZh61xgKnbjcj9Uomht38LqeihMNsY4p9KC"),
    ("aeqi_vesting", "24mJEeCHs492NGCJADvfb9zWDcqoDWNCpCYC2xAE2VBs"),
];

#[derive(Parser, Debug)]
#[command(name = "aeqi-indexer", about = "Solana log indexer for the AEQI protocol")]
struct Args {
    /// WebSocket RPC URL
    #[arg(long, env = "AEQI_INDEXER_WS", default_value = "ws://127.0.0.1:9900")]
    ws_url: String,

    /// Commitment level for live subscription (confirmed | finalized)
    #[arg(long, env = "AEQI_INDEXER_COMMITMENT", default_value = "confirmed")]
    commitment: String,

    /// SQLite database path
    #[arg(long, env = "AEQI_INDEXER_DB", default_value = "./aeqi-indexer.db")]
    db: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    info!(
        ws_url = %args.ws_url,
        commitment = %args.commitment,
        db = %args.db,
        events_known = registry::event_count(),
        "starting aeqi-indexer"
    );

    let sink = std::sync::Arc::new(sink::Sink::open(&args.db)?);
    info!(prior_events = sink.event_count()?, "sink opened");

    let commitment = match args.commitment.as_str() {
        "finalized" => CommitmentConfig::finalized(),
        _ => CommitmentConfig::confirmed(),
    };

    // Leak the client into 'static — the indexer runs for the lifetime of
    // the process so this is fine, and it lets each subscription stream
    // outlive the local function scope (required by tokio::spawn's
    // 'static bound).
    let client: &'static PubsubClient = Box::leak(Box::new(PubsubClient::new(&args.ws_url).await?));
    let mut handles = Vec::new();

    for (name, pid_str) in PROGRAMS {
        let pid = Pubkey::from_str(pid_str)?;
        let name = (*name).to_string();
        let resume_slot = sink.cursor(&name)?;
        info!(program = %name, program_id = %pid, ?resume_slot, "subscribing");

        let filter = RpcTransactionLogsFilter::Mentions(vec![pid.to_string()]);
        let cfg = RpcTransactionLogsConfig { commitment: Some(commitment) };
        let (mut sub, _unsub) = client.logs_subscribe(filter, cfg).await?;

        let sink_for_task = sink.clone();
        let handle = tokio::spawn(async move {
            while let Some(resp) = sub.next().await {
                let slot = resp.context.slot;
                if let Some(err) = &resp.value.err {
                    warn!(program = %name, slot, ?err, "tx error — skipping");
                    continue;
                }
                for line in &resp.value.logs {
                    if let Some(rest) = line.strip_prefix("Program data: ") {
                        match base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            rest,
                        ) {
                            Ok(bytes) if bytes.len() >= 8 => {
                                let payload = &bytes[8..];
                                match registry::lookup(&pid, &bytes[..8]) {
                                    Some(meta) => {
                                        let recorded = sink_for_task.record_event(
                                            meta.program,
                                            meta.event,
                                            slot,
                                            &resp.value.signature,
                                            rest,
                                        );
                                        match recorded {
                                            Ok(true) => info!(
                                                program = %meta.program,
                                                event = %meta.event,
                                                slot,
                                                sig = %resp.value.signature,
                                                payload_bytes = payload.len(),
                                                "anchor event recorded"
                                            ),
                                            Ok(false) => {
                                                // dedup hit — replay or reorg
                                            }
                                            Err(e) => warn!(?e, "sink.record_event failed"),
                                        }
                                    }
                                    None => {
                                        warn!(
                                            program = %name,
                                            slot,
                                            sig = %resp.value.signature,
                                            disc = %hex(&bytes[..8]),
                                            "unknown discriminator (event registered after indexer build?)"
                                        );
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(e) => warn!(?e, "failed to base64-decode Program data"),
                        }
                    }
                }
                if let Err(e) = sink_for_task.bump_cursor(&name, slot) {
                    warn!(?e, "sink.bump_cursor failed");
                }
            }
            warn!(program = %name, "log subscription ended");
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    s
}
