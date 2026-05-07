//! aeqi_vesting — linear cliff vesting positions for equity grants.
//!
//! Ports `modules/Vesting.module.sol`. Each VestingPosition tracks a single
//! equity grant: total_amount, start_time, cliff_time, end_time. The
//! claimable amount at any moment is:
//!
//!   if now < cliff_time:        0
//!   elif now >= end_time:       total - claimed
//!   else:                        total * (now - start) / (end - start) - claimed
//!
//! Tokens are held in a per-trust vesting vault PDA seeded
//! `[b"vesting_vault_authority", trust]`. At claim time the program signs
//! via PDA seeds to transfer the claimable amount to the recipient's ATA.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("24mJEeCHs492NGCJADvfb9zWDcqoDWNCpCYC2xAE2VBs");

#[program]
pub mod aeqi_vesting {
    use super::*;

    pub fn init(ctx: Context<InitVesting>) -> Result<()> {
        let m = &mut ctx.accounts.module_state;
        m.trust = ctx.accounts.trust.key();
        m.position_count = 0;
        m.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Create a vesting position. Caller is the grantor (treasury authority,
    /// founder, etc.). The recipient + mint + schedule are recorded; tokens
    /// must be deposited into the vesting vault separately so the program
    /// can transfer them at claim time.
    pub fn create_position(
        ctx: Context<CreatePosition>,
        position_id: [u8; 32],
        recipient: Pubkey,
        total_amount: u64,
        start_time: i64,
        cliff_time: i64,
        end_time: i64,
    ) -> Result<()> {
        require!(start_time < end_time, VestingError::InvalidSchedule);
        require!(cliff_time >= start_time, VestingError::InvalidSchedule);
        require!(cliff_time <= end_time, VestingError::InvalidSchedule);
        require!(total_amount > 0, VestingError::ZeroAmount);

        let p = &mut ctx.accounts.position;
        p.trust = ctx.accounts.trust.key();
        p.position_id = position_id;
        p.recipient = recipient;
        p.mint = ctx.accounts.mint.key();
        p.total_amount = total_amount;
        p.claimed_amount = 0;
        p.start_time = start_time;
        p.cliff_time = cliff_time;
        p.end_time = end_time;
        p.bump = ctx.bumps.position;

        let m = &mut ctx.accounts.module_state;
        m.position_count = m.position_count.checked_add(1).unwrap();

        emit!(PositionCreated {
            trust: p.trust,
            position_id,
            recipient,
            mint: p.mint,
            total_amount,
            start_time,
            cliff_time,
            end_time,
        });
        Ok(())
    }

    /// Claim vested tokens up to the current time. Permissionless to call —
    /// anyone can crank — but tokens go to the position's recipient ATA.
    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let p = &mut ctx.accounts.position;

        let vested = vested_amount_at(p, now);
        let claimable = vested.checked_sub(p.claimed_amount).unwrap();
        require!(claimable > 0, VestingError::NothingToClaim);

        let trust_key = ctx.accounts.trust.key();
        let bump = ctx.bumps.vault_authority;
        let seeds: &[&[&[u8]]] = &[&[b"vesting_vault_authority", trust_key.as_ref(), &[bump]]];

        let cpi = TransferChecked {
            from: ctx.accounts.vault.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.recipient_ta.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi,
            seeds,
        );
        transfer_checked(cpi_ctx, claimable, ctx.accounts.mint.decimals)?;

        p.claimed_amount = p.claimed_amount.checked_add(claimable).unwrap();

        emit!(Claimed {
            trust: p.trust,
            position_id: p.position_id,
            recipient: p.recipient,
            amount: claimable,
            total_claimed: p.claimed_amount,
        });
        Ok(())
    }
}

fn vested_amount_at(p: &VestingPosition, now: i64) -> u64 {
    if now < p.cliff_time {
        return 0;
    }
    if now >= p.end_time {
        return p.total_amount;
    }
    let elapsed = (now.checked_sub(p.start_time).unwrap()) as u128;
    let duration = (p.end_time.checked_sub(p.start_time).unwrap()) as u128;
    let total = p.total_amount as u128;
    let vested = total.checked_mul(elapsed).unwrap().checked_div(duration).unwrap();
    vested as u64
}

#[account]
#[derive(InitSpace)]
pub struct VestingModuleState {
    pub trust: Pubkey,
    pub position_count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct VestingPosition {
    pub trust: Pubkey,
    pub position_id: [u8; 32],
    pub recipient: Pubkey,
    pub mint: Pubkey,
    pub total_amount: u64,
    pub claimed_amount: u64,
    pub start_time: i64,
    pub cliff_time: i64,
    pub end_time: i64,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct InitVesting<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + VestingModuleState::INIT_SPACE,
        seeds = [b"vesting_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, VestingModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(position_id: [u8; 32])]
pub struct CreatePosition<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"vesting_module", trust.key().as_ref()],
        bump = module_state.bump,
    )]
    pub module_state: Account<'info, VestingModuleState>,
    #[account(
        init,
        payer = grantor,
        space = 8 + VestingPosition::INIT_SPACE,
        seeds = [b"vesting_pos", trust.key().as_ref(), position_id.as_ref()],
        bump,
    )]
    pub position: Account<'info, VestingPosition>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub grantor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    /// CHECK: trust pda
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"vesting_pos", trust.key().as_ref(), position.position_id.as_ref()],
        bump = position.bump,
    )]
    pub position: Account<'info, VestingPosition>,
    /// CHECK: program-controlled vault authority PDA. Signs the vault transfer.
    #[account(seeds = [b"vesting_vault_authority", trust.key().as_ref()], bump)]
    pub vault_authority: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = vault_authority)]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::authority = position.recipient)]
    pub recipient_ta: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[event]
pub struct PositionCreated {
    pub trust: Pubkey,
    pub position_id: [u8; 32],
    pub recipient: Pubkey,
    pub mint: Pubkey,
    pub total_amount: u64,
    pub start_time: i64,
    pub cliff_time: i64,
    pub end_time: i64,
}

#[event]
pub struct Claimed {
    pub trust: Pubkey,
    pub position_id: [u8; 32],
    pub recipient: Pubkey,
    pub amount: u64,
    pub total_claimed: u64,
}

#[error_code]
pub enum VestingError {
    #[msg("invalid schedule: start < cliff < end required")]
    InvalidSchedule,
    #[msg("vesting amount must be > 0")]
    ZeroAmount,
    #[msg("nothing to claim — fully claimed or not yet vested")]
    NothingToClaim,
}
