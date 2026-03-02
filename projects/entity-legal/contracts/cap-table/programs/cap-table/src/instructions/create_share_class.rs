use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenInterface;
use spl_token_2022::{
    extension::{
        metadata_pointer::instruction as metadata_ptr_ix,
        transfer_hook::instruction as transfer_hook_ix,
        ExtensionType,
    },
    instruction as token_ix,
    state::Mint,
};

use crate::errors::CapTableError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(class_name: String)]
pub struct CreateShareClass<'info> {
    #[account(
        mut,
        has_one = authority,
        seeds = [ENTITY_SEED, entity.name.as_bytes()],
        bump = entity.bump,
    )]
    pub entity: Account<'info, Entity>,

    #[account(
        init,
        payer = payer,
        space = ShareClass::space(class_name.len()),
        seeds = [SHARE_CLASS_SEED, entity.key().as_ref(), class_name.as_bytes()],
        bump,
    )]
    pub share_class: Account<'info, ShareClass>,

    /// The Token-2022 mint account for this share class.
    /// We initialize this manually to apply extensions before InitializeMint.
    /// CHECK: This will be initialized as a Token-2022 mint in the instruction body.
    #[account(mut)]
    pub mint: Signer<'info>,

    /// Entity authority (Squads multisig vault). Must sign to create share classes.
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    /// The transfer hook program that will enforce transfer restrictions.
    /// CHECK: We store this program ID in the mint's transfer hook extension.
    pub transfer_hook_program: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreateShareClass>,
    class_name: String,
    total_authorized: u64,
    par_value_lamports: u64,
    voting_weight: u16,
    is_transferable: bool,
    transfer_restriction: TransferRestriction,
    liquidation_preference: u64,
    requires_accreditation: bool,
    lockup_end: i64,
    max_holders: u32,
    decimals: u8,
) -> Result<()> {
    // Validate inputs.
    require!(!class_name.is_empty(), CapTableError::ClassNameEmpty);
    require!(
        class_name.len() <= MAX_CLASS_NAME_LEN,
        CapTableError::ClassNameTooLong
    );
    require!(total_authorized > 0, CapTableError::ZeroAuthorizedShares);

    let entity = &ctx.accounts.entity;
    require!(
        entity.share_class_count < 255,
        CapTableError::MaxShareClassesReached
    );

    // -----------------------------------------------------------------------
    // 1. Create the Token-2022 mint with extensions.
    //
    // Extension initialization must happen BEFORE InitializeMint2.
    // We allocate the mint account with enough space for all extensions,
    // then apply each extension, then call InitializeMint2.
    // -----------------------------------------------------------------------

    let mint_key = ctx.accounts.mint.key();
    let authority_key = ctx.accounts.authority.key();
    let transfer_hook_program_id = ctx.accounts.transfer_hook_program.key();

    // Determine which extensions to enable.
    let extensions = vec![
        ExtensionType::TransferHook,
        ExtensionType::PermanentDelegate,
        ExtensionType::MetadataPointer,
    ];

    let mint_space = ExtensionType::try_calculate_account_len::<Mint>(&extensions)?;
    let rent = &ctx.accounts.rent;
    let lamports = rent.minimum_balance(mint_space);

    // Create the mint account with enough space for extensions.
    anchor_lang::system_program::create_account(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::CreateAccount {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.mint.to_account_info(),
            },
        ),
        lamports,
        mint_space as u64,
        &ctx.accounts.token_program.key(),
    )?;

    // Initialize Transfer Hook extension.
    // The hook program is called on every transfer to enforce restrictions.
    let init_transfer_hook_ix = transfer_hook_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(transfer_hook_program_id),
    )?;
    anchor_lang::solana_program::program::invoke(
        &init_transfer_hook_ix,
        &[ctx.accounts.mint.to_account_info()],
    )?;

    // Initialize Permanent Delegate extension.
    // The entity authority can force-transfer/burn tokens for legal compliance.
    // In spl-token-2022 v4+, this is in the top-level instruction module.
    let init_perm_delegate_ix = token_ix::initialize_permanent_delegate(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,
    )?;
    anchor_lang::solana_program::program::invoke(
        &init_perm_delegate_ix,
        &[ctx.accounts.mint.to_account_info()],
    )?;

    // Initialize Metadata Pointer extension.
    // Points to the mint itself as the metadata account (inline metadata).
    let init_metadata_ptr_ix = metadata_ptr_ix::initialize(
        &ctx.accounts.token_program.key(),
        &mint_key,
        Some(authority_key),
        Some(mint_key),
    )?;
    anchor_lang::solana_program::program::invoke(
        &init_metadata_ptr_ix,
        &[ctx.accounts.mint.to_account_info()],
    )?;

    // Finally, initialize the mint itself.
    let init_mint_ix = token_ix::initialize_mint2(
        &ctx.accounts.token_program.key(),
        &mint_key,
        &authority_key,       // mint authority = entity authority
        Some(&authority_key), // freeze authority = entity authority
        decimals,
    )?;
    anchor_lang::solana_program::program::invoke(
        &init_mint_ix,
        &[ctx.accounts.mint.to_account_info()],
    )?;

    // -----------------------------------------------------------------------
    // 2. Populate the ShareClass PDA.
    // -----------------------------------------------------------------------

    let clock = Clock::get()?;
    let share_class = &mut ctx.accounts.share_class;

    share_class.entity = ctx.accounts.entity.key();
    share_class.name = class_name;
    share_class.mint = mint_key;
    share_class.total_authorized = total_authorized;
    share_class.total_issued = 0;
    share_class.par_value_lamports = par_value_lamports;
    share_class.voting_weight = voting_weight;
    share_class.is_transferable = is_transferable;
    share_class.transfer_restriction = transfer_restriction;
    share_class.liquidation_preference = liquidation_preference;
    share_class.requires_accreditation = requires_accreditation;
    share_class.lockup_end = lockup_end;
    share_class.max_holders = max_holders;
    share_class.current_holders = 0;
    share_class.created_at = clock.unix_timestamp;
    share_class.bump = ctx.bumps.share_class;

    // -----------------------------------------------------------------------
    // 3. Update the entity's share class count.
    // -----------------------------------------------------------------------

    let entity = &mut ctx.accounts.entity;
    entity.share_class_count = entity
        .share_class_count
        .checked_add(1)
        .ok_or(CapTableError::ArithmeticOverflow)?;
    entity.updated_at = clock.unix_timestamp;

    msg!(
        "Share class created: {} (mint: {}, authorized: {})",
        share_class.name,
        share_class.mint,
        share_class.total_authorized
    );

    Ok(())
}
