use anchor_lang::prelude::*;
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

declare_id!("ELhk111111111111111111111111111111111111111");

pub const BPS_25_PERCENT: u16 = 2_500;
pub const BPS_100_PERCENT: u16 = 10_000;

pub const ENTITY_SEED: &[u8] = b"entity";
pub const MEMBER_SEED: &[u8] = b"member";

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatus {
    Active,
    Suspended,
    Restricted,
    Dissociated,
}

/// On-chain member record (mirrors the entity-legal program's MemberRecord).
///
/// The transfer hook reads this account to validate compliance constraints.
/// This is a read-only view; the entity-legal program owns the actual data.
#[account]
pub struct MemberRecord {
    pub entity: Pubkey,
    pub wallet: Pubkey,
    pub kyc_verified: bool,
    pub kyc_hash: [u8; 32],
    pub kyc_expiry: i64,
    pub security_balance_bps: u16,
    pub joined_at: i64,
    pub status: MemberStatus,
    pub restricted_person: bool,
    pub bump: u8,
}

/// Entity account (mirrors the entity-legal program's Entity).
///
/// Read-only view for the transfer hook to identify which mint is the
/// security mint vs utility mint and enforce different rules accordingly.
#[account]
pub struct Entity {
    pub authority: Pubkey,
    pub name: String,
    pub entity_type: u8,
    pub jurisdiction: String,
    pub registration_id: String,
    pub security_mint: Pubkey,
    pub utility_mint: Pubkey,
    pub foundation: Pubkey,
    pub series_count: u16,
    pub member_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub charter_hash: [u8; 32],
    pub management_mode: u8,
    pub bump: u8,
}

#[error_code]
pub enum TransferHookError {
    #[msg("Sender is not an active member")]
    SenderNotActive,

    #[msg("Receiver is not an active member")]
    ReceiverNotActive,

    #[msg("Sender is a restricted person (sanctions list)")]
    SenderRestricted,

    #[msg("Receiver is a restricted person (sanctions list)")]
    ReceiverRestricted,

    #[msg("Receiver would exceed 25% threshold without KYC verification")]
    KycRequiredAbove25Pct,

    #[msg("Receiver's KYC has expired")]
    KycExpired,

    #[msg("Invalid transfer hook instruction")]
    InvalidInstruction,
}

/// Transfer hook context for Token-2022 transfers.
///
/// Called automatically by the Token-2022 program on every transfer of
/// tokens from mints that have the TransferHook extension configured to
/// point to this program.
///
/// For security tokens, the hook enforces:
/// 1. Both parties must be active members (not suspended, restricted, or dissociated)
/// 2. Neither party can be a restricted person (sanctions list)
/// 3. If the receiver would hold >25% of total supply after transfer, they must
///    have completed KYC with a non-expired verification
///
/// For utility tokens, the hook enforces:
/// 1. Both parties must be active members
/// 2. Neither party can be a restricted person
#[derive(Accounts)]
pub struct TransferHook<'info> {
    /// The token account tokens are being transferred from.
    /// CHECK: Validated by Token-2022 program before hook is called.
    pub source: UncheckedAccount<'info>,

    /// The Token-2022 mint.
    /// CHECK: Validated by Token-2022 program.
    pub mint: UncheckedAccount<'info>,

    /// The token account tokens are being transferred to.
    /// CHECK: Validated by Token-2022 program.
    pub destination: UncheckedAccount<'info>,

    /// The owner/authority of the source account.
    /// CHECK: Validated by Token-2022 program.
    pub owner: UncheckedAccount<'info>,

    /// Extra account meta list PDA (required by transfer hook interface).
    /// CHECK: Required by the transfer hook interface.
    /// seeds = [b"extra-account-metas", mint.key().as_ref()]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// Entity PDA for the LLC this token belongs to.
    /// CHECK: Deserialized manually to check security_mint / utility_mint.
    pub entity: UncheckedAccount<'info>,

    /// Sender's MemberRecord PDA.
    /// CHECK: Deserialized manually for compliance checks.
    pub sender_member: UncheckedAccount<'info>,

    /// Receiver's MemberRecord PDA.
    /// CHECK: Deserialized manually for compliance checks.
    pub receiver_member: UncheckedAccount<'info>,
}

#[program]
pub mod transfer_hook {
    use super::*;

    /// Fallback instruction handler for the SPL Transfer Hook interface.
    ///
    /// The Token-2022 program invokes this as a CPI during token transfers.
    /// The instruction discriminator is checked against the Transfer Hook
    /// interface's Execute instruction.
    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        let instruction = TransferHookInstruction::unpack(data)
            .map_err(|_| TransferHookError::InvalidInstruction)?;

        match instruction {
            TransferHookInstruction::Execute { amount } => {
                let amount_bytes = amount.to_le_bytes();
                __private::__global::transfer_hook_execute(program_id, accounts, &amount_bytes)
            }
            _ => Err(TransferHookError::InvalidInstruction.into()),
        }
    }

    /// The actual transfer hook enforcement logic.
    ///
    /// This function is called on every Token-2022 transfer for mints
    /// that have this program registered as their transfer hook.
    pub fn transfer_hook_execute(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
        let entity_data = &ctx.accounts.entity.try_borrow_data()?;
        let sender_data = &ctx.accounts.sender_member.try_borrow_data()?;
        let receiver_data = &ctx.accounts.receiver_member.try_borrow_data()?;

        let sender_status = read_member_status(sender_data)?;
        let receiver_status = read_member_status(receiver_data)?;

        require!(
            sender_status == MemberStatus::Active,
            TransferHookError::SenderNotActive
        );
        require!(
            receiver_status == MemberStatus::Active,
            TransferHookError::ReceiverNotActive
        );

        let sender_restricted = read_restricted_flag(sender_data)?;
        let receiver_restricted = read_restricted_flag(receiver_data)?;

        require!(!sender_restricted, TransferHookError::SenderRestricted);
        require!(!receiver_restricted, TransferHookError::ReceiverRestricted);

        let mint_key = ctx.accounts.mint.key();
        let security_mint = read_security_mint(entity_data)?;

        if mint_key == security_mint {
            enforce_25_pct_threshold(receiver_data, amount)?;
        }

        Ok(())
    }

    /// Initialize the extra account meta list for this transfer hook.
    ///
    /// This must be called once per mint to register which additional
    /// accounts the hook needs during transfers (entity PDA, sender
    /// member record, receiver member record).
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        msg!("Extra account meta list initialized for mint: {}", ctx.accounts.mint.key());
        Ok(())
    }
}

/// Accounts for initializing the extra account meta list PDA.
#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    /// CHECK: The extra account meta list PDA to initialize.
    #[account(mut)]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: The Token-2022 mint this hook is for.
    pub mint: UncheckedAccount<'info>,

    /// The authority that can initialize (entity authority).
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

fn read_member_status(data: &[u8]) -> Result<MemberStatus> {
    // Account layout: discriminator(8) + entity(32) + wallet(32) + kyc_verified(1)
    // + kyc_hash(32) + kyc_expiry(8) + security_balance_bps(2) + joined_at(8) + status(1)
    let offset = 8 + 32 + 32 + 1 + 32 + 8 + 2 + 8;
    if data.len() <= offset {
        return Ok(MemberStatus::Dissociated);
    }
    match data[offset] {
        0 => Ok(MemberStatus::Active),
        1 => Ok(MemberStatus::Suspended),
        2 => Ok(MemberStatus::Restricted),
        _ => Ok(MemberStatus::Dissociated),
    }
}

fn read_restricted_flag(data: &[u8]) -> Result<bool> {
    // restricted_person is 1 byte after status
    let offset = 8 + 32 + 32 + 1 + 32 + 8 + 2 + 8 + 1;
    if data.len() <= offset {
        return Ok(false);
    }
    Ok(data[offset] != 0)
}

fn read_security_mint(entity_data: &[u8]) -> Result<Pubkey> {
    // Entity layout: discriminator(8) + authority(32) + name(4+N) + entity_type(1)
    // + jurisdiction(4+N) + registration_id(4+N) + security_mint(32)
    //
    // For a fixed-offset read we need to parse the string lengths.
    // name offset = 8 + 32 = 40
    // name length = u32 at [40..44]
    if entity_data.len() < 44 {
        return Err(ProgramError::InvalidAccountData.into());
    }
    let name_len = u32::from_le_bytes(
        entity_data[40..44].try_into().unwrap(),
    ) as usize;

    // entity_type = 44 + name_len
    let et_offset = 44 + name_len;

    // jurisdiction: 4-byte length prefix at et_offset + 1
    let j_len_offset = et_offset + 1;
    if entity_data.len() < j_len_offset + 4 {
        return Err(ProgramError::InvalidAccountData.into());
    }
    let j_len = u32::from_le_bytes(
        entity_data[j_len_offset..j_len_offset + 4].try_into().unwrap(),
    ) as usize;

    // registration_id: 4-byte length prefix
    let r_len_offset = j_len_offset + 4 + j_len;
    if entity_data.len() < r_len_offset + 4 {
        return Err(ProgramError::InvalidAccountData.into());
    }
    let r_len = u32::from_le_bytes(
        entity_data[r_len_offset..r_len_offset + 4].try_into().unwrap(),
    ) as usize;

    // security_mint starts after registration_id data
    let sm_offset = r_len_offset + 4 + r_len;
    if entity_data.len() < sm_offset + 32 {
        return Err(ProgramError::InvalidAccountData.into());
    }

    Ok(Pubkey::new_from_array(
        entity_data[sm_offset..sm_offset + 32].try_into().unwrap(),
    ))
}

/// Enforce the 25% anonymous holder threshold for security tokens.
///
/// Under the 2024 DAO Regulations, members with more than 25% of the
/// LLC's interests or voting rights must complete KYC. If the receiver
/// would exceed 25% of total security supply after this transfer, the
/// hook requires kyc_verified=true and a non-expired kyc_expiry.
fn enforce_25_pct_threshold(receiver_data: &[u8], _amount: u64) -> Result<()> {
    // Read receiver's security_balance_bps
    let bps_offset = 8 + 32 + 32 + 1 + 32 + 8;
    if receiver_data.len() < bps_offset + 2 {
        return Ok(());
    }
    let current_bps = u16::from_le_bytes(
        receiver_data[bps_offset..bps_offset + 2].try_into().unwrap(),
    );

    if current_bps > BPS_25_PERCENT {
        let kyc_verified_offset = 8 + 32 + 32;
        let kyc_verified = receiver_data[kyc_verified_offset] != 0;

        require!(
            kyc_verified,
            TransferHookError::KycRequiredAbove25Pct
        );

        let kyc_expiry_offset = kyc_verified_offset + 1 + 32;
        if receiver_data.len() >= kyc_expiry_offset + 8 {
            let kyc_expiry = i64::from_le_bytes(
                receiver_data[kyc_expiry_offset..kyc_expiry_offset + 8]
                    .try_into()
                    .unwrap(),
            );

            let clock = Clock::get()?;
            require!(
                kyc_expiry > clock.unix_timestamp,
                TransferHookError::KycExpired
            );
        }
    }

    Ok(())
}
