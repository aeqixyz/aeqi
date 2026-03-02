use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenInterface;
use spl_token_2022::{
    extension::{
        metadata_pointer::instruction as metadata_ptr_ix,
        permanent_delegate::instruction as perm_delegate_ix,
        transfer_hook::instruction as transfer_hook_ix,
        ExtensionType,
    },
    instruction as token_ix,
    state::Mint,
};

use crate::errors::EntityLegalError;
use crate::state::*;

/// Creates a new series under a master Series DAO LLC.
///
/// Each series gets its own security and utility Token-2022 mints with
/// the same extension configuration as the parent entity. Series operate
/// as legally independent sub-entities with separate assets and governance.
#[derive(Accounts)]
#[instruction(series_name: String)]
pub struct CreateSeries<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        init,
        payer = payer,
        space = Series::max_space(),
        seeds = [SERIES_SEED, entity.key().as_ref(), series_name.as_bytes()],
        bump,
    )]
    pub series: Account<'info, Series>,

    /// Security token mint for this series.
    /// CHECK: Will be initialized as Token-2022 mint in handler.
    #[account(mut)]
    pub security_mint: Signer<'info>,

    /// Utility token mint for this series.
    /// CHECK: Will be initialized as Token-2022 mint in handler.
    #[account(mut)]
    pub utility_mint: Signer<'info>,

    /// Transfer hook program.
    /// CHECK: Program ID stored in mint transfer hook extension.
    pub transfer_hook_program: UncheckedAccount<'info>,

    /// Entity authority (Foundation Squads Vault 2).
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreateSeries>,
    series_name: String,
    charter_hash: [u8; 32],
    security_decimals: u8,
    utility_decimals: u8,
) -> Result<()> {
    require!(!series_name.is_empty(), EntityLegalError::SeriesNameEmpty);
    require!(
        series_name.len() <= MAX_SERIES_NAME_LEN,
        EntityLegalError::SeriesNameTooLong
    );

    let entity = &ctx.accounts.entity;
    require!(entity.series_count < u16::MAX, EntityLegalError::MaxSeriesReached);

    let authority_key = ctx.accounts.authority.key();
    let hook_program_id = ctx.accounts.transfer_hook_program.key();
    let rent = &ctx.accounts.rent;

    initialize_series_security_mint(&ctx, authority_key, hook_program_id, rent, security_decimals)?;
    initialize_series_utility_mint(&ctx, authority_key, hook_program_id, rent, utility_decimals)?;

    let clock = Clock::get()?;
    let series = &mut ctx.accounts.series;

    series.parent_entity = ctx.accounts.entity.key();
    series.name = series_name;
    series.security_mint = ctx.accounts.security_mint.key();
    series.utility_mint = ctx.accounts.utility_mint.key();
    series.member_count = 0;
    series.created_at = clock.unix_timestamp;
    series.charter_hash = charter_hash;
    series.bump = ctx.bumps.series;

    let entity = &mut ctx.accounts.entity;
    entity.series_count = entity
        .series_count
        .checked_add(1)
        .ok_or(EntityLegalError::ArithmeticOverflow)?;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Series created: {} under entity {} (series #{})",
        series.name,
        entity.name,
        entity.series_count
    );

    Ok(())
}

fn initialize_series_security_mint(
    ctx: &Context<CreateSeries>,
    authority_key: Pubkey,
    transfer_hook_program_id: Pubkey,
    rent: &Rent,
    decimals: u8,
) -> Result<()> {
    let mint_key = ctx.accounts.security_mint.key();
    let extensions = vec![
        ExtensionType::TransferHook,
        ExtensionType::PermanentDelegate,
        ExtensionType::MetadataPointer,
    ];
    let mint_space = ExtensionType::try_calculate_account_len::<Mint>(&extensions)?;
    let lamports = rent.minimum_balance(mint_space);

    anchor_lang::system_program::create_account(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::CreateAccount {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.security_mint.to_account_info(),
            },
        ),
        lamports,
        mint_space as u64,
        &ctx.accounts.token_program.key(),
    )?;

    let ix = transfer_hook_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(transfer_hook_program_id),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    let ix = perm_delegate_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    let ix = metadata_ptr_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(mint_key),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    let ix = token_ix::initialize_mint2(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
        Some(&authority_key),
        decimals,
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    Ok(())
}

fn initialize_series_utility_mint(
    ctx: &Context<CreateSeries>,
    authority_key: Pubkey,
    transfer_hook_program_id: Pubkey,
    rent: &Rent,
    decimals: u8,
) -> Result<()> {
    let mint_key = ctx.accounts.utility_mint.key();
    let extensions = vec![
        ExtensionType::TransferHook,
        ExtensionType::MetadataPointer,
    ];
    let mint_space = ExtensionType::try_calculate_account_len::<Mint>(&extensions)?;
    let lamports = rent.minimum_balance(mint_space);

    anchor_lang::system_program::create_account(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::CreateAccount {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.utility_mint.to_account_info(),
            },
        ),
        lamports,
        mint_space as u64,
        &ctx.accounts.token_program.key(),
    )?;

    let ix = transfer_hook_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(transfer_hook_program_id),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.utility_mint.to_account_info()],
    )?;

    let ix = metadata_ptr_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(mint_key),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.utility_mint.to_account_info()],
    )?;

    let ix = token_ix::initialize_mint2(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
        Some(&authority_key),
        decimals,
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix,
        &[ctx.accounts.utility_mint.to_account_info()],
    )?;

    Ok(())
}
