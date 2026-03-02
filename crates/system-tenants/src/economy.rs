use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;

use crate::config::TierConfig;

/// Economy balance for a tenant.
#[derive(Debug, Clone, Serialize)]
pub struct EconomyBalance {
    pub summons: i64,
    pub mana: i64,
    pub summons_max: u32,
    pub mana_max: u32,
}

/// Get the current balance for a tenant, creating the row if it doesn't exist.
pub fn get_balance(conn: &Connection, tenant_id: &str, tier: &TierConfig) -> Result<EconomyBalance> {
    ensure_economy_row(conn, tenant_id, tier)?;
    regenerate_if_needed(conn, tenant_id, tier)?;

    let mut stmt = conn.prepare(
        "SELECT summons, mana FROM economy WHERE tenant_id = ?1"
    )?;
    let (summons, mana): (i64, i64) = stmt.query_row(
        rusqlite::params![tenant_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).context("economy row not found")?;

    Ok(EconomyBalance {
        summons,
        mana,
        summons_max: tier.summons_per_day,
        mana_max: tier.mana_per_day,
    })
}

/// Ensure the economy row exists for a tenant.
fn ensure_economy_row(conn: &Connection, tenant_id: &str, tier: &TierConfig) -> Result<()> {
    let now = Utc::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT OR IGNORE INTO economy (tenant_id, summons, mana, last_regen) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tenant_id, tier.summons_per_day, tier.mana_per_day, now],
    )?;
    Ok(())
}

/// If a new UTC day has started since last regen, reset balances to daily max.
fn regenerate_if_needed(conn: &Connection, tenant_id: &str, tier: &TierConfig) -> Result<()> {
    let last_regen: String = conn.query_row(
        "SELECT last_regen FROM economy WHERE tenant_id = ?1",
        rusqlite::params![tenant_id],
        |row| row.get(0),
    ).context("economy row not found")?;

    let today = Utc::now().format("%Y-%m-%d").to_string();
    if last_regen != today {
        conn.execute(
            "UPDATE economy SET summons = ?1, mana = ?2, last_regen = ?3 WHERE tenant_id = ?4",
            rusqlite::params![tier.summons_per_day, tier.mana_per_day, today, tenant_id],
        )?;
        // Log the regeneration
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO economy_log (tenant_id, currency, delta, reason, created_at) VALUES (?1, 'summons', ?2, 'daily_regen', ?3)",
            rusqlite::params![tenant_id, tier.summons_per_day as i64, now],
        )?;
        conn.execute(
            "INSERT INTO economy_log (tenant_id, currency, delta, reason, created_at) VALUES (?1, 'mana', ?2, 'daily_regen', ?3)",
            rusqlite::params![tenant_id, tier.mana_per_day as i64, now],
        )?;
    }
    Ok(())
}

/// Spend summons. Returns true if successful, false if insufficient balance.
pub fn spend_summons(conn: &Connection, tenant_id: &str, amount: i64, tier: &TierConfig) -> Result<bool> {
    ensure_economy_row(conn, tenant_id, tier)?;
    regenerate_if_needed(conn, tenant_id, tier)?;

    let current: i64 = conn.query_row(
        "SELECT summons FROM economy WHERE tenant_id = ?1",
        rusqlite::params![tenant_id],
        |row| row.get(0),
    )?;

    if current < amount {
        return Ok(false);
    }

    conn.execute(
        "UPDATE economy SET summons = summons - ?1 WHERE tenant_id = ?2",
        rusqlite::params![amount, tenant_id],
    )?;

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO economy_log (tenant_id, currency, delta, reason, created_at) VALUES (?1, 'summons', ?2, 'pull', ?3)",
        rusqlite::params![tenant_id, -amount, now],
    )?;

    Ok(true)
}

/// Spend mana. Returns true if successful, false if insufficient balance.
pub fn spend_mana(conn: &Connection, tenant_id: &str, amount: i64, tier: &TierConfig) -> Result<bool> {
    ensure_economy_row(conn, tenant_id, tier)?;
    regenerate_if_needed(conn, tenant_id, tier)?;

    let current: i64 = conn.query_row(
        "SELECT mana FROM economy WHERE tenant_id = ?1",
        rusqlite::params![tenant_id],
        |row| row.get(0),
    )?;

    if current < amount {
        return Ok(false);
    }

    conn.execute(
        "UPDATE economy SET mana = mana - ?1 WHERE tenant_id = ?2",
        rusqlite::params![amount, tenant_id],
    )?;

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO economy_log (tenant_id, currency, delta, reason, created_at) VALUES (?1, 'mana', ?2, 'chat', ?3)",
        rusqlite::params![tenant_id, -amount, now],
    )?;

    Ok(true)
}
