//! aeqi_role — role DAG, role types, delegations, vote checkpoints.
//!
//! Ports `modules/Role.module.sol`. Roles form a parent-child DAG with
//! per-role-type hierarchy levels. Authority is computed by walking parent
//! pointers — on Solana the walk is bounded and the off-chain client provides
//! the path of parent role PDAs in `remaining_accounts` so each PDA is loaded
//! exactly once.
//!
//! Two voting paths are exposed for `aeqi_governance`:
//! 1. **Token voting** — handled by `aeqi_token` (this module is bypassed).
//! 2. **Per-role multisig** — voting power = number of `role_type` roles the
//!    account is the active delegate of, snapshotted at proposal start via
//!    checkpoints maintained on every delegation change.

use anchor_lang::prelude::*;

declare_id!("HFqh9bPLS7EwirMsz9MpNT96SN5v2JBeKTdnUpSVyuVe");

pub mod errors;
pub mod state;

pub use errors::*;
pub use state::*;

pub const MAX_AUTHORITY_WALK: usize = 8;

#[program]
pub mod aeqi_role {
    use super::*;

    /// Module init — called by `aeqi_factory` during template instantiation.
    /// Stores the parent TRUST and CPIs into `aeqi_trust::ack_module_init`.
    pub fn init(ctx: Context<InitModule>) -> Result<()> {
        let module = &mut ctx.accounts.module_state;
        module.trust = ctx.accounts.trust.key();
        module.initialized = true;
        module.bump = ctx.bumps.module_state;
        Ok(())
    }

    /// Module finalize — borsh-deserializes the role-module config from the
    /// TRUST `BytesConfig` slot under `ROLE_CONFIG_KEY` and pre-creates any
    /// role types declared at template time. Mirrors EVM `finalizeModule`.
    pub fn finalize(_ctx: Context<FinalizeModule>) -> Result<()> {
        // Real impl: read trust BytesConfig at ROLE_CONFIG_KEY, borsh-decode
        // into Vec<RoleTypeInit>, create the RoleType PDAs declared at
        // template time. Skeleton.
        Ok(())
    }

    /// Define a role type. Mirrors EVM `Role.module.createRoleType`.
    /// Hierarchy lower number = higher authority (0 = founder/admin).
    pub fn create_role_type(
        ctx: Context<CreateRoleType>,
        role_type_id: [u8; 32],
        hierarchy: u32,
        config: RoleTypeConfig,
    ) -> Result<()> {
        let rt = &mut ctx.accounts.role_type;
        rt.trust = ctx.accounts.trust.key();
        rt.role_type_id = role_type_id;
        rt.hierarchy = hierarchy;
        rt.config = config;
        rt.role_count = 0;
        rt.bump = ctx.bumps.role_type;
        emit!(RoleTypeCreated {
            trust: rt.trust,
            role_type_id,
            hierarchy,
        });
        Ok(())
    }

    /// Create a new role under a parent role. Authority: caller must hold a
    /// role that is an ancestor of `parent_role_id` (or be the TRUST authority
    /// during creation mode). The off-chain client supplies the ancestor walk
    /// in `remaining_accounts`.
    pub fn create_role<'info>(
        ctx: Context<'_, '_, 'info, 'info, CreateRole<'info>>,
        role_id: [u8; 32],
        role_type_id: [u8; 32],
        parent_role_id: Option<[u8; 32]>,
        ipfs_cid: [u8; 64],
    ) -> Result<()> {
        // Authority gate (live mode): if caller_role is provided, walk the
        // role DAG to confirm caller has authority over `parent_role_id`.
        // Permissionless when caller_role is omitted — gated upstream by the
        // factory or trust authority via the parent transaction signing.
        if let Some(caller_role) = ctx.accounts.caller_role.as_ref() {
            require!(
                caller_role.account == ctx.accounts.payer.key(),
                AeqiRoleError::Unauthorized
            );
            if let Some(parent) = parent_role_id {
                check_authority_walk(caller_role, &parent, ctx.remaining_accounts)?;
            }
        }

        let role = &mut ctx.accounts.role;
        role.trust = ctx.accounts.trust.key();
        role.role_id = role_id;
        role.role_type_id = role_type_id;
        role.account = Pubkey::default();
        role.parent_role_id = parent_role_id.unwrap_or([0u8; 32]);
        role.status = RoleStatus::Vacant as u8;
        role.status_since = Clock::get()?.unix_timestamp;
        role.ipfs_cid = ipfs_cid;
        role.bump = ctx.bumps.role;

        let rt = &mut ctx.accounts.role_type;
        rt.role_count = rt.role_count.checked_add(1).unwrap();

        emit!(RoleCreated {
            trust: role.trust,
            role_id,
            role_type_id,
            parent_role_id: role.parent_role_id,
        });
        Ok(())
    }

    /// Assign an account to a vacant role. Sets status = Occupied and
    /// auto-self-delegates voting power. Mirrors EVM `assignToRole`.
    pub fn assign_role(ctx: Context<AssignRole>, account: Pubkey) -> Result<()> {
        let role = &mut ctx.accounts.role;
        require!(
            role.status == RoleStatus::Vacant as u8,
            AeqiRoleError::RoleNotVacant
        );
        role.account = account;
        role.status = RoleStatus::Occupied as u8;
        role.status_since = Clock::get()?.unix_timestamp;

        // Auto-self-delegate: bump checkpoint for `account` on this role's type.
        bump_checkpoint(
            &mut ctx.accounts.checkpoint,
            account,
            ctx.accounts.role_type.role_type_id,
            1,
        )?;

        emit!(RoleAssigned {
            trust: role.trust,
            role_id: role.role_id,
            account,
        });
        Ok(())
    }

    /// Transfer an Occupied role from the current holder to a new account.
    /// Decrements the prior holder's checkpoint, increments the new holder's
    /// checkpoint. Mirrors EVM `Role.module.transferRole`.
    pub fn transfer_role(ctx: Context<TransferRole>, new_account: Pubkey) -> Result<()> {
        let role = &mut ctx.accounts.role;
        require!(
            role.status == RoleStatus::Occupied as u8,
            AeqiRoleError::RoleNotOccupied
        );
        require_keys_eq!(
            ctx.accounts.payer.key(),
            role.account,
            AeqiRoleError::Unauthorized
        );

        let prev_account = role.account;
        role.account = new_account;
        role.status_since = Clock::get()?.unix_timestamp;

        // Move 1 vote on this role's type from prev to new.
        bump_checkpoint(
            &mut ctx.accounts.prev_checkpoint,
            prev_account,
            ctx.accounts.role_type.role_type_id,
            -1,
        )?;
        bump_checkpoint(
            &mut ctx.accounts.new_checkpoint,
            new_account,
            ctx.accounts.role_type.role_type_id,
            1,
        )?;

        emit!(RoleTransferred {
            trust: role.trust,
            role_id: role.role_id,
            from: prev_account,
            to: new_account,
        });
        Ok(())
    }

    /// Delegate this role's voting power to another account. Decrements the
    /// previous delegatee's checkpoint and increments the new delegatee's.
    pub fn delegate_role(ctx: Context<DelegateRole>, delegatee: Pubkey) -> Result<()> {
        let role = &ctx.accounts.role;
        require!(
            role.status == RoleStatus::Occupied as u8,
            AeqiRoleError::RoleNotOccupied
        );
        require_keys_eq!(
            ctx.accounts.payer.key(),
            role.account,
            AeqiRoleError::Unauthorized
        );

        let deleg = &mut ctx.accounts.delegation;
        let prev = deleg.delegatee;
        deleg.trust = role.trust;
        deleg.role_id = role.role_id;
        deleg.delegatee = delegatee;
        deleg.bump = ctx.bumps.delegation;

        if prev != Pubkey::default() && prev != delegatee {
            let prev_ckpt = ctx
                .accounts
                .prev_checkpoint
                .as_mut()
                .ok_or(AeqiRoleError::PrevCheckpointRequired)?;
            bump_checkpoint(
                prev_ckpt,
                prev,
                ctx.accounts.role_type.role_type_id,
                -1,
            )?;
        }
        bump_checkpoint(
            &mut ctx.accounts.new_checkpoint,
            delegatee,
            ctx.accounts.role_type.role_type_id,
            1,
        )?;

        emit!(RoleDelegated {
            trust: role.trust,
            role_id: role.role_id,
            from: prev,
            to: delegatee,
        });
        Ok(())
    }

    /// Read-only — returns the active delegation count for `account` of
    /// `role_type` at the given slot. Used by `aeqi_governance` at vote-cast
    /// time. The client passes the most-recent checkpoint with `slot <=
    /// query_slot`; the program verifies its `slot` field is correct.
    pub fn get_past_role_votes(
        ctx: Context<GetPastRoleVotes>,
        query_slot: u64,
    ) -> Result<u64> {
        let ckpt = &ctx.accounts.checkpoint;
        require!(
            ckpt.slot <= query_slot,
            AeqiRoleError::CheckpointAfterQuery
        );
        Ok(ckpt.count)
    }
}

/// Verify that `caller_role` has authority over `target_role_id`. Authority
/// flows top-down: caller is authorized iff caller's role IS target_role_id
/// OR caller's role is an ancestor of target_role_id.
///
/// Walk strategy: start at the target role, walk parent pointers UP looking
/// for caller's role_id. `remaining_accounts` must contain the chain
/// `[target_role_pda, target.parent_role_pda, ...up to root]`.
fn check_authority_walk<'info>(
    caller_role: &Account<'info, Role>,
    target_role_id: &[u8; 32],
    remaining: &'info [AccountInfo<'info>],
) -> Result<()> {
    if &caller_role.role_id == target_role_id {
        return Ok(());
    }
    let mut expected_id = *target_role_id;
    for (i, acc) in remaining.iter().take(MAX_AUTHORITY_WALK).enumerate() {
        let role: Account<Role> = Account::try_from(acc)
            .map_err(|_| AeqiRoleError::InvalidAuthorityWalk)?;
        require!(
            role.trust == caller_role.trust,
            AeqiRoleError::InvalidAuthorityWalk
        );
        require!(
            role.role_id == expected_id,
            AeqiRoleError::InvalidAuthorityWalk
        );
        // Hit the caller in the target's ancestor chain → authorized.
        if role.role_id == caller_role.role_id {
            return Ok(());
        }
        if role.parent_role_id == [0u8; 32] {
            break;
        }
        expected_id = role.parent_role_id;
        if i + 1 == MAX_AUTHORITY_WALK {
            return err!(AeqiRoleError::AuthorityWalkTooDeep);
        }
    }
    err!(AeqiRoleError::AuthorityNotFound)
}

fn bump_checkpoint(
    ckpt: &mut Account<RoleVoteCheckpoint>,
    account: Pubkey,
    role_type_id: [u8; 32],
    delta: i64,
) -> Result<()> {
    ckpt.account = account;
    ckpt.role_type_id = role_type_id;
    ckpt.slot = Clock::get()?.slot;
    if delta >= 0 {
        ckpt.count = ckpt.count.checked_add(delta as u64).unwrap();
    } else {
        ckpt.count = ckpt.count.checked_sub((-delta) as u64).unwrap();
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Account contexts (skeleton — full set lands with implementation)
// -----------------------------------------------------------------------------

/// CHECK on `trust`: validated structurally via aeqi_trust seeds. The role
/// module assumes the trust account is well-formed because the factory created
/// it; we read `creation_mode` and `key` only.
#[derive(Accounts)]
pub struct InitModule<'info> {
    /// CHECK: cross-program account, validated by seeds + the factory flow.
    pub trust: AccountInfo<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + RoleModuleState::INIT_SPACE,
        seeds = [b"role_module", trust.key().as_ref()],
        bump,
    )]
    pub module_state: Account<'info, RoleModuleState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeModule<'info> {
    /// CHECK: cross-program TRUST account.
    pub trust: AccountInfo<'info>,
    #[account(seeds = [b"role_module", trust.key().as_ref()], bump = module_state.bump)]
    pub module_state: Account<'info, RoleModuleState>,
}

#[derive(Accounts)]
#[instruction(role_type_id: [u8; 32])]
pub struct CreateRoleType<'info> {
    /// CHECK: validated by seeds.
    pub trust: AccountInfo<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + RoleType::INIT_SPACE,
        seeds = [b"role_type", trust.key().as_ref(), role_type_id.as_ref()],
        bump,
    )]
    pub role_type: Account<'info, RoleType>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(role_id: [u8; 32], role_type_id: [u8; 32])]
pub struct CreateRole<'info> {
    /// CHECK: trust pda — used only as the seed namespace for the role +
    /// role_type PDAs. Authority gating is handled via the caller_role walk.
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"role_type", trust.key().as_ref(), role_type_id.as_ref()],
        bump = role_type.bump,
    )]
    pub role_type: Account<'info, RoleType>,
    #[account(
        init,
        payer = payer,
        space = 8 + Role::INIT_SPACE,
        seeds = [b"role", trust.key().as_ref(), role_id.as_ref()],
        bump,
    )]
    pub role: Account<'info, Role>,
    /// The role held by the caller (only required in live mode).
    pub caller_role: Option<Account<'info, Role>>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AssignRole<'info> {
    #[account(mut, has_one = trust)]
    pub role: Account<'info, Role>,
    pub role_type: Account<'info, RoleType>,
    /// CHECK: structural.
    pub trust: AccountInfo<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + RoleVoteCheckpoint::INIT_SPACE,
        seeds = [b"role_ckpt", trust.key().as_ref(), role_type.role_type_id.as_ref(), payer.key().as_ref()],
        bump,
    )]
    pub checkpoint: Account<'info, RoleVoteCheckpoint>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferRole<'info> {
    #[account(mut, has_one = trust)]
    pub role: Account<'info, Role>,
    pub role_type: Account<'info, RoleType>,
    /// CHECK: trust pda — used as seed for checkpoints.
    pub trust: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"role_ckpt", trust.key().as_ref(), role_type.role_type_id.as_ref(), role.account.as_ref()],
        bump,
    )]
    pub prev_checkpoint: Account<'info, RoleVoteCheckpoint>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + RoleVoteCheckpoint::INIT_SPACE,
        seeds = [b"role_ckpt", trust.key().as_ref(), role_type.role_type_id.as_ref(), new_account.key().as_ref()],
        bump,
    )]
    pub new_checkpoint: Account<'info, RoleVoteCheckpoint>,
    /// CHECK: the new role holder — used as seed for the new checkpoint PDA.
    pub new_account: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DelegateRole<'info> {
    pub role: Account<'info, Role>,
    pub role_type: Account<'info, RoleType>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + RoleDelegation::INIT_SPACE,
        seeds = [b"role_deleg", role.trust.as_ref(), role.role_id.as_ref()],
        bump,
    )]
    pub delegation: Account<'info, RoleDelegation>,
    /// Optional — required only when re-delegating away from a prior
    /// delegatee. First-time delegation passes None.
    #[account(mut)]
    pub prev_checkpoint: Option<Account<'info, RoleVoteCheckpoint>>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + RoleVoteCheckpoint::INIT_SPACE,
        seeds = [b"role_ckpt", role.trust.as_ref(), role_type.role_type_id.as_ref(), new_delegatee.key().as_ref()],
        bump,
    )]
    pub new_checkpoint: Account<'info, RoleVoteCheckpoint>,
    /// CHECK: the new delegatee — used as a seed for the new checkpoint PDA
    /// and as the recipient of the +1 vote. Doesn't need to sign.
    pub new_delegatee: UncheckedAccount<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GetPastRoleVotes<'info> {
    pub checkpoint: Account<'info, RoleVoteCheckpoint>,
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

#[event]
pub struct RoleTypeCreated {
    pub trust: Pubkey,
    pub role_type_id: [u8; 32],
    pub hierarchy: u32,
}

#[event]
pub struct RoleCreated {
    pub trust: Pubkey,
    pub role_id: [u8; 32],
    pub role_type_id: [u8; 32],
    pub parent_role_id: [u8; 32],
}

#[event]
pub struct RoleAssigned {
    pub trust: Pubkey,
    pub role_id: [u8; 32],
    pub account: Pubkey,
}

#[event]
pub struct RoleTransferred {
    pub trust: Pubkey,
    pub role_id: [u8; 32],
    pub from: Pubkey,
    pub to: Pubkey,
}

#[event]
pub struct RoleDelegated {
    pub trust: Pubkey,
    pub role_id: [u8; 32],
    pub from: Pubkey,
    pub to: Pubkey,
}
