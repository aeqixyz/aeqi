import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiVesting } from "../target/types/aeqi_vesting";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountInstruction,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { expect } from "chai";

describe("aeqi_vesting", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiVesting as Program<AeqiVesting>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;
  let vaultAuthority: PublicKey;
  let mint: PublicKey;
  let vaultAta: PublicKey;
  let recipientAta: PublicKey;

  before(async () => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_module"), fakeTrust.toBuffer()],
      program.programId,
    );
    [vaultAuthority] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_vault_authority"), fakeTrust.toBuffer()],
      program.programId,
    );

    mint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      6,
      Keypair.generate(),
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    vaultAta = getAssociatedTokenAddressSync(
      mint,
      vaultAuthority,
      true,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(
      new anchor.web3.Transaction().add(
        createAssociatedTokenAccountInstruction(
          provider.wallet.publicKey,
          vaultAta,
          vaultAuthority,
          mint,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      ),
    );

    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      vaultAta,
      provider.wallet.publicKey,
      10_000,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    recipientAta = getAssociatedTokenAddressSync(
      mint,
      provider.wallet.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(
      new anchor.web3.Transaction().add(
        createAssociatedTokenAccountInstruction(
          provider.wallet.publicKey,
          recipientAta,
          provider.wallet.publicKey,
          mint,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      ),
    );
  });

  it("init creates the vesting module state", async () => {
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.vestingModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.positionCount.toString()).to.eq("0");
  });

  it("create_position + claim — fully-vested grant transfers entire amount", async () => {
    const positionId = new Uint8Array(32);
    positionId[0] = 0xf1;

    const [posPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_pos"), fakeTrust.toBuffer(), Buffer.from(positionId)],
      program.programId,
    );

    // Schedule entirely in the past — position is fully vested at creation.
    const now = Math.floor(Date.now() / 1000);
    const start = now - 1000;
    const cliff = now - 500;
    const end = now - 100;

    await program.methods
      .createPosition(
        Array.from(positionId),
        provider.wallet.publicKey,
        new anchor.BN(10_000),
        new anchor.BN(start),
        new anchor.BN(cliff),
        new anchor.BN(end),
        new anchor.BN(0), // no contribution
        PublicKey.default,
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        position: posPda,
        mint,
        grantor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const pos = await program.account.vestingPosition.fetch(posPda);
    expect(pos.totalAmount.toString()).to.eq("10000");
    expect(pos.claimedAmount.toString()).to.eq("0");

    // Claim — fully vested → transfer all 10000
    await program.methods
      .claim()
      .accounts({
        trust: fakeTrust,
        position: posPda,
        vaultAuthority,
        mint,
        vault: vaultAta,
        recipientTa: recipientAta,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    const recipient = await getAccount(
      provider.connection,
      recipientAta,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );
    expect(recipient.amount.toString()).to.eq("10000");

    const posPost = await program.account.vestingPosition.fetch(posPda);
    expect(posPost.claimedAmount.toString()).to.eq("10000");

    // Second claim should fail — nothing left
    let threw = false;
    try {
      await program.methods
        .claim()
        .accounts({
          trust: fakeTrust,
          position: posPda,
          vaultAuthority,
          mint,
          vault: vaultAta,
          recipientTa: recipientAta,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/NothingToClaim/);
    }
    expect(threw).to.eq(true);
  });

  it("mark_fdv_milestone fully unlocks a pre-cliff position", async () => {
    // Create a position entirely in the future (pre-cliff). Normally
    // claim would fail with NothingToClaim. After mark_fdv_milestone the
    // grant fully unlocks regardless of schedule.
    const positionId = new Uint8Array(32);
    positionId[0] = 0xfd;
    positionId[1] = 0xff;

    const [posPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_pos"), fakeTrust.toBuffer(), Buffer.from(positionId)],
      program.programId,
    );

    const now = Math.floor(Date.now() / 1000);
    const start = now + 1000;
    const cliff = now + 2000;
    const end = now + 5000;

    await program.methods
      .createPosition(
        Array.from(positionId),
        provider.wallet.publicKey,
        new anchor.BN(7777),
        new anchor.BN(start),
        new anchor.BN(cliff),
        new anchor.BN(end),
        new anchor.BN(0),
        PublicKey.default,
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        position: posPda,
        mint,
        grantor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Pre-cliff claim should fail
    let preFdvFailed = false;
    try {
      await program.methods
        .claim()
        .accounts({
          trust: fakeTrust,
          position: posPda,
          vaultAuthority,
          mint,
          vault: vaultAta,
          recipientTa: recipientAta,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .rpc();
    } catch (e: any) {
      preFdvFailed = true;
      expect(e.toString()).to.match(/NothingToClaim/);
    }
    expect(preFdvFailed).to.eq(true);

    // Top up the vault to cover the FDV-unlocked grant (the previous tests
    // drained it; ensure 7777 is available)
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      vaultAta,
      provider.wallet.publicKey,
      7777,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    const recipientPre = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);

    // Mark FDV milestone hit
    await program.methods
      .markFdvMilestone()
      .accounts({
        position: posPda,
        grantor: provider.wallet.publicKey,
      })
      .rpc();

    let pos = await program.account.vestingPosition.fetch(posPda);
    expect(pos.fdvMilestoneUnlocked).to.eq(true);

    // Claim — should now succeed with full 7777 amount
    await program.methods
      .claim()
      .accounts({
        trust: fakeTrust,
        position: posPda,
        vaultAuthority,
        mint,
        vault: vaultAta,
        recipientTa: recipientAta,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    const recipientPost = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect((recipientPost.amount - recipientPre.amount).toString()).to.eq("7777");

    pos = await program.account.vestingPosition.fetch(posPda);
    expect(pos.claimedAmount.toString()).to.eq("7777");

    // Re-marking should fail (one-way flag)
    let reThrew = false;
    try {
      await program.methods
        .markFdvMilestone()
        .accounts({
          position: posPda,
          grantor: provider.wallet.publicKey,
        })
        .rpc();
    } catch (e: any) {
      reThrew = true;
      expect(e.toString()).to.match(/AlreadyUnlocked/);
    }
    expect(reThrew).to.eq(true);
  });

  it("pay_contribution gates claims — recipient must burn before claiming", async () => {
    // Position: fully-vested schedule (in the past), contribution_required = 500.
    // Without pay_contribution: claim fails ContributionUnpaid.
    // After pay_contribution (burns 500 of contribution_mint from recipient):
    // claim succeeds with full vested amount.
    const positionId = new Uint8Array(32);
    positionId[0] = 0xc0;
    positionId[1] = 0xde;

    const [posPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_pos"), fakeTrust.toBuffer(), Buffer.from(positionId)],
      program.programId,
    );

    const now = Math.floor(Date.now() / 1000);
    const start = now - 1000;
    const cliff = now - 500;
    const end = now - 100;
    const TOTAL = 10_000;
    const CONTRIBUTION = 500;

    // Use the same mint for asset + contribution (test wallet is mint authority).
    await program.methods
      .createPosition(
        Array.from(positionId),
        provider.wallet.publicKey,
        new anchor.BN(TOTAL),
        new anchor.BN(start),
        new anchor.BN(cliff),
        new anchor.BN(end),
        new anchor.BN(CONTRIBUTION),
        mint,
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        position: posPda,
        mint,
        grantor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Top up vault to cover claim
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      vaultAta,
      provider.wallet.publicKey,
      TOTAL,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Pre-payment: claim must fail
    let preFailed = false;
    try {
      await program.methods
        .claim()
        .accounts({
          trust: fakeTrust,
          position: posPda,
          vaultAuthority,
          mint,
          vault: vaultAta,
          recipientTa: recipientAta,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .rpc();
    } catch (e: any) {
      preFailed = true;
      expect(e.toString()).to.match(/ContributionUnpaid/);
    }
    expect(preFailed).to.eq(true);

    // Mint contribution amount to recipient
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      recipientAta,
      provider.wallet.publicKey,
      CONTRIBUTION,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    const recipientPre = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);

    // Pay contribution (burns CONTRIBUTION tokens from recipientAta)
    await program.methods
      .payContribution()
      .accounts({
        position: posPda,
        contributionMint: mint,
        recipientContributionTa: recipientAta,
        recipient: provider.wallet.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    let pos = await program.account.vestingPosition.fetch(posPda);
    expect(pos.contributionPaid).to.eq(true);

    // Recipient just lost CONTRIBUTION tokens to burn
    const recipientPostBurn = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect((recipientPre.amount - recipientPostBurn.amount).toString()).to.eq(String(CONTRIBUTION));

    // Now claim should succeed and transfer full TOTAL
    await program.methods
      .claim()
      .accounts({
        trust: fakeTrust,
        position: posPda,
        vaultAuthority,
        mint,
        vault: vaultAta,
        recipientTa: recipientAta,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    const recipientPostClaim = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect((recipientPostClaim.amount - recipientPostBurn.amount).toString()).to.eq(String(TOTAL));

    pos = await program.account.vestingPosition.fetch(posPda);
    expect(pos.claimedAmount.toString()).to.eq(String(TOTAL));
  });

  it("create_position rejects pre-cliff claims (NothingToClaim)", async () => {
    const positionId = new Uint8Array(32);
    positionId[0] = 0xf2;

    const [posPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_pos"), fakeTrust.toBuffer(), Buffer.from(positionId)],
      program.programId,
    );

    // Schedule entirely in the future — pre-cliff
    const now = Math.floor(Date.now() / 1000);
    const start = now + 1000;
    const cliff = now + 2000;
    const end = now + 5000;

    await program.methods
      .createPosition(
        Array.from(positionId),
        provider.wallet.publicKey,
        new anchor.BN(5_000),
        new anchor.BN(start),
        new anchor.BN(cliff),
        new anchor.BN(end),
        new anchor.BN(0),
        PublicKey.default,
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        position: posPda,
        mint,
        grantor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    let threw = false;
    try {
      await program.methods
        .claim()
        .accounts({
          trust: fakeTrust,
          position: posPda,
          vaultAuthority,
          mint,
          vault: vaultAta,
          recipientTa: recipientAta,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/NothingToClaim/);
    }
    expect(threw).to.eq(true);
  });
});
