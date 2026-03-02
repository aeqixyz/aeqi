use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

declare_id!("4tJrurgXmsGqd9nvf4a66Yu3PqwahCUh3Bx5cTKcJbtL");

// ---------------------------------------------------------------------------
// Account structures (mirrors of cap table state, for deserialization)
// ---------------------------------------------------------------------------

/// Minimal representation of a MemberRecord for transfer hook validation.
/// Must match the cap table program's MemberRecord layout (after 8-byte discriminator).
pub struct MemberRecordData {
    pub entity: Pubkey,
    pub wallet: Pubkey,
    pub kyc_verified: bool,
    pub _kyc_hash: [u8; 32],
    pub accredited: bool,
    pub _joined_at: i64,
    pub status: u8, // 0 = Active, 1 = Suspended, 2 = Removed
    pub _bump: u8,
}

impl MemberRecordData {
    pub const MIN_SIZE: usize = 8 + 32 + 32 + 1 + 32 + 1 + 8 + 1 + 1;

    pub fn try_deserialize(data: &[u8]) -> Result<Self> {
        // Skip 8-byte Anchor discriminator.
        if data.len() < Self::MIN_SIZE {
            return Err(ProgramError::InvalidAccountData.into());
        }
        let d = &data[8..];
        let mut offset = 0;

        let entity = Pubkey::try_from(&d[offset..offset + 32])
            .map_err(|_| ProgramError::InvalidAccountData)?;
        offset += 32;

        let wallet = Pubkey::try_from(&d[offset..offset + 32])
            .map_err(|_| ProgramError::InvalidAccountData)?;
        offset += 32;

        let kyc_verified = d[offset] != 0;
        offset += 1;

        let mut kyc_hash = [0u8; 32];
        kyc_hash.copy_from_slice(&d[offset..offset + 32]);
        offset += 32;

        let accredited = d[offset] != 0;
        offset += 1;

        let joined_at = i64::from_le_bytes(
            d[offset..offset + 8]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        );
        offset += 8;

        let status = d[offset];
        offset += 1;

        let bump = d[offset];

        Ok(Self {
            entity,
            wallet,
            kyc_verified,
            _kyc_hash: kyc_hash,
            accredited,
            _joined_at: joined_at,
            status,
            _bump: bump,
        })
    }

    pub fn is_active(&self) -> bool {
        self.status == 0
    }
}

/// Minimal representation of a ShareClass for transfer hook validation.
pub struct ShareClassData {
    pub _entity: Pubkey,
    pub requires_accreditation: bool,
    pub lockup_end: i64,
    pub is_transferable: bool,
}

impl ShareClassData {
    /// Parse the ShareClass from raw account data.
    /// Layout after discriminator (8 bytes):
    ///   entity: 32
    ///   name: 4 (len) + variable
    ///   mint: 32
    ///   total_authorized: 8
    ///   total_issued: 8
    ///   par_value_lamports: 8
    ///   voting_weight: 2
    ///   is_transferable: 1
    ///   transfer_restriction: 1 + 8 (enum discriminant + largest variant data)
    ///   liquidation_preference: 8
    ///   requires_accreditation: 1
    ///   lockup_end: 8
    pub fn try_deserialize(data: &[u8]) -> Result<Self> {
        if data.len() < 8 + 32 + 4 {
            return Err(ProgramError::InvalidAccountData.into());
        }
        let d = &data[8..]; // skip discriminator
        let mut offset = 0;

        let entity = Pubkey::try_from(&d[offset..offset + 32])
            .map_err(|_| ProgramError::InvalidAccountData)?;
        offset += 32;

        // Read name length to skip past variable-length name.
        let name_len = u32::from_le_bytes(
            d[offset..offset + 4]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        );
        offset += 4 + name_len as usize; // skip name bytes

        // Bounds check after variable-length field.
        if d.len() < offset + 32 + 8 + 8 + 8 + 2 + 1 + 9 + 8 + 1 + 8 {
            return Err(ProgramError::InvalidAccountData.into());
        }

        // Skip: mint (32) + total_authorized (8) + total_issued (8) + par_value_lamports (8)
        //       + voting_weight (2)
        offset += 32 + 8 + 8 + 8 + 2;

        let is_transferable = d[offset] != 0;
        offset += 1;

        // Skip transfer_restriction enum (1 discriminant + up to 8 bytes data).
        offset += 1 + 8;

        // Skip liquidation_preference (8).
        offset += 8;

        let requires_accreditation = d[offset] != 0;
        offset += 1;

        let lockup_end = i64::from_le_bytes(
            d[offset..offset + 8]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        );

        Ok(Self {
            _entity: entity,
            requires_accreditation,
            lockup_end,
            is_transferable,
        })
    }
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[error_code]
pub enum TransferHookError {
    #[msg("Sender is not KYC verified")]
    SenderKycRequired,

    #[msg("Receiver is not KYC verified")]
    ReceiverKycRequired,

    #[msg("Sender is not an active member")]
    SenderNotActive,

    #[msg("Receiver is not an active member")]
    ReceiverNotActive,

    #[msg("Receiver does not have accredited investor status")]
    AccreditationRequired,

    #[msg("Transfers are locked until the lock-up period ends")]
    LockupActive,

    #[msg("Transfers are not permitted for this share class")]
    TransfersNotPermitted,

    #[msg("Invalid sender member record")]
    InvalidSenderMember,

    #[msg("Invalid receiver member record")]
    InvalidReceiverMember,

    #[msg("Invalid share class account")]
    InvalidShareClass,

    #[msg("Extra account meta list has insufficient space")]
    InsufficientMetaListSpace,
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

#[program]
pub mod transfer_hook {
    use super::*;

    /// Initialize the ExtraAccountMetaList for this mint.
    ///
    /// Token-2022's transfer hook interface requires an ExtraAccountMetaList PDA
    /// that tells the token program which additional accounts to pass to the hook
    /// on every transfer.
    ///
    /// The extra accounts needed during transfer are:
    ///   1. Sender's MemberRecord PDA (from cap_table program)
    ///   2. Receiver's MemberRecord PDA (from cap_table program)
    ///   3. ShareClass PDA (from cap_table program)
    ///
    /// This instruction must be called once per mint after the mint is created.
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        // Initialize the ExtraAccountMetaList PDA data.
        // We write a minimal valid header. The actual extra accounts
        // (sender_member, receiver_member, share_class) are resolved
        // and appended by the client when constructing transfer instructions.
        let account_info = &ctx.accounts.extra_account_meta_list;
        let mut data = account_info.try_borrow_mut_data()?;

        // Write a minimal ExtraAccountMetaList header.
        // The spl-transfer-hook-interface expects:
        //   - 8-byte discriminator (ArrayDiscriminator for ExtraAccountMetaList)
        //   - 4-byte u32 LE length field
        // We set length to 0 — the client is responsible for passing
        // the additional accounts when calling transfer.
        //
        // ArrayDiscriminator value for ExtraAccountMetaList:
        // First 8 bytes of SHA-256 of "ExtraAccountMetaList::execute"
        let discriminator: [u8; 8] = [
            0x08, 0xc9, 0x7b, 0xce, 0x23, 0x0c, 0x05, 0x12,
        ];

        if data.len() >= 12 {
            data[..8].copy_from_slice(&discriminator);
            data[8..12].copy_from_slice(&0u32.to_le_bytes());
        }

        msg!("Transfer hook extra account meta list initialized for mint: {}", ctx.accounts.mint.key());
        Ok(())
    }

    /// The transfer hook — called by Token-2022 on every transfer.
    ///
    /// Enforces the following compliance checks:
    /// 1. Both sender and receiver must be KYC-verified, active members
    /// 2. If the share class requires accreditation, the receiver must be accredited
    /// 3. If a lock-up period is active, the transfer is rejected
    /// 4. If the share class is marked as non-transferable, the transfer is rejected
    pub fn transfer_hook(ctx: Context<TransferHookCtx>, _amount: u64) -> Result<()> {
        // Deserialize the extra accounts.
        let sender_member_info = &ctx.accounts.sender_member;
        let receiver_member_info = &ctx.accounts.receiver_member;
        let share_class_info = &ctx.accounts.share_class;

        // Validate and deserialize sender member record.
        let sender_data = sender_member_info.try_borrow_data()?;
        let sender_member = MemberRecordData::try_deserialize(&sender_data)
            .map_err(|_| TransferHookError::InvalidSenderMember)?;

        require!(sender_member.is_active(), TransferHookError::SenderNotActive);
        require!(
            sender_member.kyc_verified,
            TransferHookError::SenderKycRequired
        );

        // Validate and deserialize receiver member record.
        let receiver_data = receiver_member_info.try_borrow_data()?;
        let receiver_member = MemberRecordData::try_deserialize(&receiver_data)
            .map_err(|_| TransferHookError::InvalidReceiverMember)?;

        require!(
            receiver_member.is_active(),
            TransferHookError::ReceiverNotActive
        );
        require!(
            receiver_member.kyc_verified,
            TransferHookError::ReceiverKycRequired
        );

        // Validate and deserialize share class.
        let share_class_data_raw = share_class_info.try_borrow_data()?;
        let share_class = ShareClassData::try_deserialize(&share_class_data_raw)
            .map_err(|_| TransferHookError::InvalidShareClass)?;

        // Check: transfers must be permitted for this share class.
        require!(
            share_class.is_transferable,
            TransferHookError::TransfersNotPermitted
        );

        // Check: accredited investor requirement.
        if share_class.requires_accreditation {
            require!(
                receiver_member.accredited,
                TransferHookError::AccreditationRequired
            );
        }

        // Check: lock-up period.
        if share_class.lockup_end > 0 {
            let clock = Clock::get()?;
            require!(
                clock.unix_timestamp > share_class.lockup_end,
                TransferHookError::LockupActive
            );
        }

        msg!("Transfer hook: compliance checks passed");
        Ok(())
    }

    /// Fallback instruction handler required by the transfer hook interface.
    /// Routes to the transfer_hook handler when called via CPI from Token-2022.
    ///
    /// The Token-2022 program invokes the transfer hook using a specific
    /// instruction discriminator. This fallback catches that CPI call and
    /// dispatches it to our `transfer_hook` handler.
    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        // The transfer hook interface uses the Execute instruction discriminator.
        // We simply route all fallback calls to our transfer_hook handler.
        // The Anchor framework will validate the accounts via the TransferHookCtx.
        msg!("Transfer hook fallback: routing to transfer_hook handler");
        __private::__global::transfer_hook(program_id, accounts, data)
    }
}

// ---------------------------------------------------------------------------
// Account contexts
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    /// The Token-2022 mint this hook is configured for.
    pub mint: InterfaceAccount<'info, Mint>,

    /// The ExtraAccountMetaList PDA.
    /// Seeds: [b"extra-account-metas", mint.key()]
    /// CHECK: Initialized in the instruction body. We write the header manually.
    #[account(
        init,
        payer = authority,
        space = 16, // discriminator (8) + length u32 (4) + padding
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: AccountInfo<'info>,

    /// The authority that can initialize this hook (typically entity authority).
    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferHookCtx<'info> {
    /// Source token account (sender).
    /// CHECK: Validated by Token-2022 program before calling the hook.
    pub source: AccountInfo<'info>,

    /// The mint being transferred.
    pub mint: InterfaceAccount<'info, Mint>,

    /// Destination token account (receiver).
    /// CHECK: Validated by Token-2022 program before calling the hook.
    pub destination: AccountInfo<'info>,

    /// Owner/authority of the source token account.
    /// CHECK: Validated by Token-2022 program.
    pub owner: AccountInfo<'info>,

    /// The ExtraAccountMetaList PDA.
    /// CHECK: Validated by seeds constraint.
    #[account(
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: AccountInfo<'info>,

    // --- Extra accounts passed by the client ---

    /// Sender's MemberRecord PDA (from cap_table program).
    /// CHECK: Deserialized and validated in the instruction body.
    pub sender_member: AccountInfo<'info>,

    /// Receiver's MemberRecord PDA (from cap_table program).
    /// CHECK: Deserialized and validated in the instruction body.
    pub receiver_member: AccountInfo<'info>,

    /// ShareClass PDA (from cap_table program).
    /// CHECK: Deserialized and validated in the instruction body.
    pub share_class: AccountInfo<'info>,
}
