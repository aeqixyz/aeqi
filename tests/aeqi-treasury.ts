import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiTreasury } from "../target/types/aeqi_treasury";
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

describe("aeqi_treasury", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiTreasury as Program<AeqiTreasury>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;
  let vaultAuthority: PublicKey;
  let mint: PublicKey;
  let vaultAta: PublicKey;
  let recipientAta: PublicKey;

  before(async () => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("treasury_module"), fakeTrust.toBuffer()],
      program.programId,
    );
    [vaultAuthority] = PublicKey.findProgramAddressSync(
      [Buffer.from("treasury_vault_authority"), fakeTrust.toBuffer()],
      program.programId,
    );

    // Create a fresh Token-2022 mint with the test wallet as mint authority.
    // The test owns minting power; the treasury PDA owns the vault account.
    const mintKeypair = Keypair.generate();
    mint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      6, // USDC-like decimals
      mintKeypair,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Create vault ATA owned by vault_authority PDA.
    vaultAta = getAssociatedTokenAddressSync(
      mint,
      vaultAuthority,
      true, // PDA owner
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const vaultIx = createAssociatedTokenAccountInstruction(
      provider.wallet.publicKey,
      vaultAta,
      vaultAuthority,
      mint,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(new anchor.web3.Transaction().add(vaultIx));

    // Mint 5000 tokens INTO the vault.
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      vaultAta,
      provider.wallet.publicKey,
      5000,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Recipient ATA (owned by test wallet) for withdraw destination.
    recipientAta = getAssociatedTokenAddressSync(
      mint,
      provider.wallet.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const recIx = createAssociatedTokenAccountInstruction(
      provider.wallet.publicKey,
      recipientAta,
      provider.wallet.publicKey,
      mint,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(new anchor.web3.Transaction().add(recIx));
  });

  it("init creates the treasury module with the configured authority", async () => {
    await program.methods
      .init(provider.wallet.publicKey)
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.treasuryModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.treasuryAuthority.toBase58()).to.eq(
      provider.wallet.publicKey.toBase58(),
    );
  });

  it("withdraw transfers from the vault PDA to a recipient ATA", async () => {
    // Verify vault has 5000 before withdraw
    let vaultPre = await getAccount(provider.connection, vaultAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vaultPre.amount.toString()).to.eq("5000");

    await program.methods
      .withdraw(new anchor.BN(2000))
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        vaultAuthority,
        mint,
        vault: vaultAta,
        recipientTa: recipientAta,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    const vaultPost = await getAccount(provider.connection, vaultAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vaultPost.amount.toString()).to.eq("3000");

    const recPost = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect(recPost.amount.toString()).to.eq("2000");
  });

  it("deposit increases vault balance + emits typed event", async () => {
    // Mint 3000 to recipientAta first so we have something to deposit FROM
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      mint,
      recipientAta,
      provider.wallet.publicKey,
      3000,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    const vaultPre = await getAccount(provider.connection, vaultAta, undefined, TOKEN_2022_PROGRAM_ID);
    const recPre = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);

    await program.methods
      .deposit(new anchor.BN(1500))
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        vaultAuthority,
        mint,
        vault: vaultAta,
        depositorTa: recipientAta,
        depositor: provider.wallet.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    const vaultPost = await getAccount(provider.connection, vaultAta, undefined, TOKEN_2022_PROGRAM_ID);
    const recPost = await getAccount(provider.connection, recipientAta, undefined, TOKEN_2022_PROGRAM_ID);

    expect((vaultPost.amount - vaultPre.amount).toString()).to.eq("1500");
    expect((recPre.amount - recPost.amount).toString()).to.eq("1500");
  });

  it("withdraw rejects unauthorized signer", async () => {
    const intruder = Keypair.generate();
    // Fund intruder so they can pay for tx
    const sig = await provider.connection.requestAirdrop(intruder.publicKey, 1e9);
    await provider.connection.confirmTransaction(sig);

    let threw = false;
    try {
      await program.methods
        .withdraw(new anchor.BN(100))
        .accounts({
          trust: fakeTrust,
          moduleState: modulePda,
          vaultAuthority,
          mint,
          vault: vaultAta,
          recipientTa: recipientAta,
          authority: intruder.publicKey,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([intruder])
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/Unauthorized/);
    }
    expect(threw).to.eq(true);
  });
});
