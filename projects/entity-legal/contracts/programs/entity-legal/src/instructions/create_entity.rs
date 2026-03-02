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

/// Creates a new Marshall Islands DAO LLC on-chain.
///
/// Initializes the Entity PDA plus both Token-2022 mints (security + utility)
/// with the appropriate extensions. The Foundation's Squads vault is set as
/// the entity authority and permanent delegate on the security token.
#[derive(Accounts)]
#[instruction(entity_id: String, name: String, jurisdiction: String, registration_id: String)]
pub struct CreateEntity<'info> {
    #[account(
        init,
        payer = payer,
        space = Entity::max_space(),
        seeds = [ENTITY_SEED, entity_id.as_bytes()],
        bump,
    )]
    pub entity: Account<'info, Entity>,

    /// Foundation's Squads multisig Vault 2 (entity authority).
    /// CHECK: Stored as authority. Squads program validates internally.
    pub authority: UncheckedAccount<'info>,

    /// Foundation entity PDA reference.
    /// CHECK: Stored for reference. Not validated here.
    pub foundation: UncheckedAccount<'info>,

    /// Security token Token-2022 mint. Initialized manually for extensions.
    /// CHECK: Will be initialized as Token-2022 mint in handler.
    #[account(mut)]
    pub security_mint: Signer<'info>,

    /// Utility token Token-2022 mint. Initialized manually for extensions.
    /// CHECK: Will be initialized as Token-2022 mint in handler.
    #[account(mut)]
    pub utility_mint: Signer<'info>,

    /// Transfer hook program enforcing the 25% threshold.
    /// CHECK: Program ID stored in mint transfer hook extension.
    pub transfer_hook_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreateEntity>,
    _entity_id: String,
    name: String,
    entity_type: EntityType,
    jurisdiction: String,
    registration_id: String,
    charter_hash: [u8; 32],
    management_mode: ManagementMode,
    security_decimals: u8,
    utility_decimals: u8,
) -> Result<()> {
    require!(!name.is_empty(), EntityLegalError::EntityNameEmpty);
    require!(name.len() <= MAX_NAME_LEN, EntityLegalError::EntityNameTooLong);
    require!(
        jurisdiction.len() <= MAX_JURISDICTION_LEN,
        EntityLegalError::JurisdictionTooLong
    );
    require!(
        registration_id.len() <= MAX_REGISTRATION_ID_LEN,
        EntityLegalError::RegistrationIdTooLong
    );

    let authority_key = ctx.accounts.authority.key();
    let transfer_hook_program_id = ctx.accounts.transfer_hook_program.key();
    let rent = &ctx.accounts.rent;

    initialize_security_mint(
        &ctx,
        authority_key,
        transfer_hook_program_id,
        rent,
        security_decimals,
    )?;

    initialize_utility_mint(
        &ctx,
        authority_key,
        transfer_hook_program_id,
        rent,
        utility_decimals,
    )?;

    let clock = Clock::get()?;
    let entity = &mut ctx.accounts.entity;

    entity.authority = authority_key;
    entity.name = name;
    entity.entity_type = entity_type;
    entity.jurisdiction = jurisdiction;
    entity.registration_id = registration_id;
    entity.security_mint = ctx.accounts.security_mint.key();
    entity.utility_mint = ctx.accounts.utility_mint.key();
    entity.foundation = ctx.accounts.foundation.key();
    entity.series_count = 0;
    entity.member_count = 0;
    entity.created_at = clock.unix_timestamp;
    entity.updated_at = clock.unix_timestamp;
    entity.charter_hash = charter_hash;
    entity.management_mode = management_mode;
    entity.bump = ctx.bumps.entity;

    msg!(
        "Entity created: {} (type: {:?}, authority: {})",
        entity.name,
        entity.entity_type,
        entity.authority
    );

    Ok(())
}

fn initialize_security_mint(
    ctx: &Context<CreateEntity>,
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

    let ix_hook = transfer_hook_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(transfer_hook_program_id),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_hook,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    let ix_delegate = perm_delegate_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_delegate,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    let ix_metadata = metadata_ptr_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(mint_key),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_metadata,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    let ix_init = token_ix::initialize_mint2(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
        Some(&authority_key),
        decimals,
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_init,
        &[ctx.accounts.security_mint.to_account_info()],
    )?;

    Ok(())
}

fn initialize_utility_mint(
    ctx: &Context<CreateEntity>,
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

    let ix_hook = transfer_hook_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(transfer_hook_program_id),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_hook,
        &[ctx.accounts.utility_mint.to_account_info()],
    )?;

    let ix_metadata = metadata_ptr_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(mint_key),
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_metadata,
        &[ctx.accounts.utility_mint.to_account_info()],
    )?;

    let ix_init = token_ix::initialize_mint2(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
        Some(&authority_key),
        decimals,
    )?;
    anchor_lang::solana_program::program::invoke(
        &ix_init,
        &[ctx.accounts.utility_mint.to_account_info()],
    )?;

    Ok(())
}
