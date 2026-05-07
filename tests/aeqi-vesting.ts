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
