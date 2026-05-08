//! aeqi_token — cap-table token, SPL Token-2022 mint authority.
//!
//! Ports `modules/Token.module.sol`. Each TRUST gets one Token-2022 mint
//! whose authority is a PDA of this program seeded
//! `[b"token_authority", trust]`. Module finalize decodes
//! `(name, symbol, decimals, max_supply, allocations[])` from
//! TRUST `BytesConfig` slot `TOKEN_TRUST_CONFIG_KEY` and creates the mint +
//! initial allocation accounts.
//!
//! This iteration: `init` stores the TokenModuleState PDA. Mint creation via
//! Token-2022 CPI lands as `create_mint` in the next iteration.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn, mint_to, Burn, Mint, MintTo, TokenAccount, TokenInterface,
};

declare_id!("V9WiXaeayA8KTyVAEEG1rAuPQ28G6NEwzSCmzZNZv6z");

/// aeqi_trust program id — used for cross-program account read of the
/// BytesConfig PDA written by the factory before finalize.
pub const AEQI_TRUST_ID: Pubkey =
    anchor_lang::pubkey!("AF9cqzwiGCf2XHtLXyKJwToPaJghmEaHa9VQJ1zjoUHs");

/// Stable PDA-key suffix the factory writes the token's borsh-encoded
/// `TokenInitConfig` blob under, in trust's BytesConfig slot. Each module
/// owns a distinct prefix byte so config-bytes PDAs never collide.
pub const TOKEN_CONFIG_KEY: [u8; 32] = {
    let mut k = [0u8; 32];
    k[0] = 1;
    k
};

/// Mirror of `aeqi_trust::BytesConfig` field layout. Borsh-deserialized from
/// the raw account bytes after skipping the 8-byte Anchor discriminator;
/// matches the cross-program account-read pattern used in
/// `aeqi_governance::cast_vote_role`.
#[derive(AnchorDeserialize, AnchorSerialize)]
pub struct BytesConfigData {
    pub trust: Pubkey,
    pub key: [u8; 32],
    pub value: Vec<u8>,
    pub bump: u8,
}

/// Borsh-serialized config the factory writes to the trust BytesConfig slot
/// at `TOKEN_CONFIG_KEY` before invoking `aeqi_token::finalize`. Mirrors the
/// EVM `Token.module.finalizeModule` `abi.decode(getBytesConfig(KEY), ...)`
/// dispatch pattern.
#[derive(AnchorDeserialize, AnchorSerialize)]
pub struct TokenInitConfig {
    pub decimals: u8,
    pub max_supply_cap: u64,
}

#[program]
pub mod aeqi_token {
    use super::*;

    /// Module init — called by the factory (or directly by the user during
    /// company spawn). Creates the TokenModuleState PDA that anchors all
    /// subsequent token operations to this trust.
    pub fn init(ctx: Context<InitToken>) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        module.trust = ctx.accounts.trust.key();
        module.mint = Pubkey::default(); // set by create_mint
        module.initialized = ModuleInitState::Initialized as u8;
        module.bump = ctx.bumps.module_state;
        emit!(TokenModuleInitialized {
            trust: module.trust,
            module_state: ctx.accounts.module_state.key(),
        });
        Ok(())
    }

    /// Module finalize — decodes the config bytes the factory wrote into the
    /// trust's BytesConfig slot under `TOKEN_CONFIG_KEY`. Mirrors EVM
    /// `Token.module.finalizeModule`'s
    /// `abi.decode(getBytesConfig(TOKEN_TRUST_CONFIG), (decimals, maxSupply))`
    /// step. Cross-program account read — the BytesConfig PDA's owner is
    /// validated against AEQI_TRUST_ID, then the 8-byte discriminator is
    /// skipped and the bytes are borsh-deserialized into the mirror struct.
    pub fn finalize(ctx: Context<FinalizeToken>) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        require!(
            module.initialized == ModuleInitState::Initialized as u8,
            TokenError::NotInitialized
        );

        let cfg_acct = &ctx.accounts.bytes_config;
        require_keys_eq!(*cfg_acct.owner, AEQI_TRUST_ID, TokenError::InvalidConfig);

        let data = cfg_acct.try_borrow_data()?;
        require!(data.len() >= 8, TokenError::InvalidConfig);
        let cfg = BytesConfigData::try_from_slice(&data[8..])
            .map_err(|_| error!(TokenError::InvalidConfig))?;
        require_keys_eq!(
            cfg.trust,
            ctx.accounts.trust.key(),
            TokenError::InvalidConfig
        );
        require!(cfg.key == TOKEN_CONFIG_KEY, TokenError::InvalidConfig);

        let init_cfg = TokenInitConfig::try_from_slice(&cfg.value)
            .map_err(|_| error!(TokenError::InvalidConfig))?;

        module.decimals = init_cfg.decimals;
        module.max_supply_cap = init_cfg.max_supply_cap;
        module.initialized = ModuleInitState::Finalized as u8;
        Ok(())
    }

    /// Burn cap-table tokens. The token account owner signs; no program
    /// authority needed (Token-2022 burn requires the owner's signature).
    /// Used for redemption, exit, buyback, vesting clawback (when the vault
    /// is owned by a vesting PDA).
    pub fn burn_tokens(ctx: Context<BurnTokens>, amount: u64) -> Result<()> {
        let module = &ctx.accounts.module_state;
        require!(
            module.mint == ctx.accounts.mint.key(),
            TokenError::MintMismatch
        );

        let cpi_accounts = Burn {
            mint: ctx.accounts.mint.to_account_info(),
            from: ctx.accounts.owner_ta.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );
        burn(cpi_ctx, amount)?;

        emit!(TokensBurned {
            trust: module.trust,
            mint: module.mint,
            owner_ta: ctx.accounts.owner_ta.key(),
            amount,
        });
        Ok(())
    }

    /// Issue cap-table tokens. Mints `amount` tokens to `recipient_ta` via
    /// CPI into Token-2022, signing with the program-controlled mint
    /// authority PDA seeds. No off-chain key holds mint authority.
    ///
    /// Supply cap: when `module_state.max_supply_cap > 0` the post-mint
    /// total supply is checked against the cap (cap=0 means "uncapped",
    /// the pre-finalize default).
    pub fn mint_tokens(ctx: Context<MintTokens>, amount: u64) -> Result<()> {
        let module = &ctx.accounts.module_state;
        require!(
            module.mint == ctx.accounts.mint.key(),
            TokenError::MintMismatch
        );

        if module.max_supply_cap > 0 {
            let current_supply = ctx.accounts.mint.supply;
            let new_supply = current_supply
                .checked_add(amount)
                .ok_or(error!(TokenError::SupplyCapExceeded))?;
            require!(
                new_supply <= module.max_supply_cap,
                TokenError::SupplyCapExceeded
            );
        }

        let trust_key = ctx.accounts.trust.key();
        let bump = ctx.bumps.mint_authority;
        let seeds: &[&[&[u8]]] = &[&[b"token_authority", trust_key.as_ref(), &[bump]]];

        let cpi_accounts = MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.recipient_ta.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            seeds,
        );
        mint_to(cpi_ctx, amount)?;

        emit!(TokensMinted {
            trust: module.trust,
            mint: module.mint,
            recipient_ta: ctx.accounts.recipient_ta.key(),
            amount,
        });
        Ok(())
    }

    /// Create the SPL Token-2022 mint for this TRUST. Mint address is a PDA
    /// seeded `[b"mint", trust]` so callers can derive it deterministically.
    /// Authority for the mint is another PDA seeded
    /// `[b"token_authority", trust]`, owned by this program — only this
    /// program can mint or freeze.
    pub fn create_mint(ctx: Context<CreateMint>, decimals: u8) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        // Mint creation is valid post-init *and* post-finalize. The factory
        // pipeline finalizes the module before user-driven create_mint runs,
        // so requiring strict Initialized would lock out the canonical flow.
        require!(
            module.initialized != ModuleInitState::Pending as u8,
            TokenError::NotInitialized
        );
        require!(
            module.mint == Pubkey::default(),
            TokenError::MintAlreadyCreated
        );
        module.mint = ctx.accounts.mint.key();
        emit!(MintCreated {
            trust: module.trust,
            mint: module.mint,
            decimals,
        });
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct TokenModuleState {
    pub trust: Pubkey,
    pub mint: Pubkey,
    pub initialized: u8,
    /// Mint decimals — populated by `finalize` from the BytesConfig blob.
    pub decimals: u8,
    /// Authoritative supply cap from `TokenInitConfig`. `mint_tokens` will
    /// (next iteration) gate against this once minting is wired through it.
    pub max_supply_cap: u64,
    pub bump: u8,
}

#[repr(u8)]
pub enum ModuleInitState {
    Pending = 0,
    Initialized = 1,
    Finalized = 2,
}

#[derive(Accounts)]
pub struct InitToken<'info> {
    /// CHECK: structurally validated by the parent trust PDA derivation; this
    /// module just records the trust key so subsequent ix can authorize against
    /// it.
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + TokenModuleState::INIT_SPACE,
        seeds = [b"token_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeToken<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"token_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    /// CHECK: cross-program BytesConfig PDA owned by aeqi_trust. Anchor
    /// enforces the seed derivation under the foreign program id; finalize's
    /// body validates the account's data layout + owner.
    #[account(
        seeds = [b"cfg_bytes", trust.key().as_ref(), TOKEN_CONFIG_KEY.as_ref()],
        bump,
        seeds::program = AEQI_TRUST_ID,
    )]
    pub bytes_config: UncheckedAccount<'info>,
}

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateMint<'info> {
    /// CHECK: trust pda — used as the seed namespace.
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"token_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    /// CHECK: program-controlled PDA mint authority. Only this program (via
    /// signer seeds) can mint or freeze the cap-table token.
    #[account(seeds = [b"token_authority", trust.key().as_ref()], bump)]
    pub mint_authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        mint::decimals = decimals,
        mint::authority = mint_authority,
        mint::token_program = token_program,
        seeds = [b"mint", trust.key().as_ref()],
        bump,
    )]
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BurnTokens<'info> {
    /// CHECK: trust pda — used as the seed namespace.
    pub trust: UncheckedAccount<'info>,
    #[account(
        seeds = [b"token_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    #[account(mut, seeds = [b"mint", trust.key().as_ref()], bump)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub owner_ta: InterfaceAccount<'info, TokenAccount>,
    pub owner: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct MintTokens<'info> {
    /// CHECK: trust pda — used as the seed namespace.
    pub trust: UncheckedAccount<'info>,
    #[account(
        seeds = [b"token_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, TokenModuleState>,
    /// CHECK: program-controlled PDA mint authority. Signed via signer seeds.
    #[account(seeds = [b"token_authority", trust.key().as_ref()], bump)]
    pub mint_authority: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"mint", trust.key().as_ref()], bump)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub recipient_ta: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[event]
pub struct TokenModuleInitialized {
    pub trust: Pubkey,
    pub module_state: Pubkey,
}

#[event]
pub struct MintCreated {
    pub trust: Pubkey,
    pub mint: Pubkey,
    pub decimals: u8,
}

#[event]
pub struct TokensMinted {
    pub trust: Pubkey,
    pub mint: Pubkey,
    pub recipient_ta: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TokensBurned {
    pub trust: Pubkey,
    pub mint: Pubkey,
    pub owner_ta: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum TokenError {
    #[msg("token module not yet initialized")]
    NotInitialized,
    #[msg("mint already created for this trust")]
    MintAlreadyCreated,
    #[msg("mint account does not match the module's recorded mint")]
    MintMismatch,
    #[msg("BytesConfig PDA missing, malformed, or wrong owner")]
    InvalidConfig,
    #[msg("mint would exceed max_supply_cap from TokenInitConfig")]
    SupplyCapExceeded,
}
