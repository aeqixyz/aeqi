//! aeqi_token — cap-table token, SPL Token-2022 mint authority.
//!
//! Ports `modules/Token.module.sol`. Each TRUST gets one Token-2022 mint
//! whose authority is a PDA of this program seeded
//! `[b"token_mint", trust]`. Module finalize decodes
//! `(name, symbol, decimals, max_supply, allocations[])` from
//! TRUST `BytesConfig` slot `TOKEN_TRUST_CONFIG_KEY` and creates the mint +
//! initial allocation accounts.
//!
//! Token-2022 transfer hooks reserved for compliance / vesting locks
//! (vesting tokens, freeze authority) — wiring lands with `aeqi_vesting`.
//!
//! Skeleton.

use anchor_lang::prelude::*;

declare_id!("V9WiXaeayA8KTyVAEEG1rAuPQ28G6NEwzSCmzZNZv6z");

#[program]
pub mod aeqi_token {
    use super::*;

    pub fn init(_ctx: Context<InitModule>) -> Result<()> {
        Ok(())
    }

    pub fn finalize(_ctx: Context<FinalizeModule>) -> Result<()> {
        // TODO: decode trust BytesConfig[TOKEN_TRUST_CONFIG_KEY] → create mint
        // via Token-2022 CPI → mint allocations to recipient ATAs.
        Ok(())
    }

    pub fn mint(_ctx: Context<MintTokens>) -> Result<()> {
        // ACL-gated mint. CPI into Token-2022.
        Ok(())
    }

    pub fn burn(_ctx: Context<BurnTokens>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitModule<'info> {
    /// CHECK: trust pda.
    pub trust: AccountInfo<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeModule<'info> {
    /// CHECK: trust pda.
    pub trust: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct MintTokens<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct BurnTokens<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
}
