use anchor_lang::prelude::*;

use crate::errors::CapTableError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(entity_id: String, name: String, jurisdiction: String, registration_id: String)]
pub struct InitializeEntity<'info> {
    #[account(
        init,
        payer = payer,
        space = Entity::space(name.len(), jurisdiction.len(), registration_id.len()),
        seeds = [ENTITY_SEED, entity_id.as_bytes()],
        bump,
    )]
    pub entity: Account<'info, Entity>,

    /// The Squads multisig vault that will govern this entity.
    /// Stored as the authority — all privileged operations require this signer.
    /// CHECK: This is the Squads vault PDA. We store it but do not validate its
    /// internal structure; the Squads program owns that validation.
    pub authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeEntity>,
    _entity_id: String,
    name: String,
    jurisdiction: String,
    registration_id: String,
    charter_hash: [u8; 32],
) -> Result<()> {
    // Validate inputs.
    require!(!name.is_empty(), CapTableError::EntityNameEmpty);
    require!(name.len() <= MAX_NAME_LEN, CapTableError::EntityNameTooLong);
    require!(
        jurisdiction.len() <= MAX_JURISDICTION_LEN,
        CapTableError::JurisdictionTooLong
    );
    require!(
        registration_id.len() <= MAX_REGISTRATION_ID_LEN,
        CapTableError::RegistrationIdTooLong
    );

    let clock = Clock::get()?;
    let entity = &mut ctx.accounts.entity;

    entity.authority = ctx.accounts.authority.key();
    entity.name = name;
    entity.jurisdiction = jurisdiction;
    entity.registration_id = registration_id;
    entity.share_class_count = 0;
    entity.member_count = 0;
    entity.created_at = clock.unix_timestamp;
    entity.updated_at = clock.unix_timestamp;
    entity.charter_hash = charter_hash;
    entity.rofr_active = false;
    entity.bump = ctx.bumps.entity;

    msg!(
        "Entity initialized: {} (authority: {})",
        entity.name,
        entity.authority
    );

    Ok(())
}
