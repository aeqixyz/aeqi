import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiToken } from "../target/types/aeqi_token";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountInstruction,
  getAccount,
} from "@solana/spl-token";
import { expect } from "chai";

describe("aeqi_token", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiToken as Program<AeqiToken>;

  it("init creates a TokenModuleState PDA bound to a trust", async () => {
    // Use a synthetic trust pubkey — for this isolated test the trust PDA
    // doesn't need to be a real aeqi_trust account; the module just records
    // its address.
    const fakeTrust = Keypair.generate().publicKey;

    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_module"), fakeTrust.toBuffer()],
      program.programId,
    );

    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const state = await program.account.tokenModuleState.fetch(moduleStatePda);
    expect(state.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(state.initialized).to.eq(1); // ModuleInitState::Initialized
    expect(state.mint.toBase58()).to.eq(PublicKey.default.toBase58());
  });

  it("create_mint creates a Token-2022 mint as a PDA", async () => {
    const fakeTrust = Keypair.generate().publicKey;

    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_module"), fakeTrust.toBuffer()],
      program.programId,
    );

    // Init the module state first
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const [mintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_authority"), fakeTrust.toBuffer()],
      program.programId,
    );
    const [mintPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("mint"), fakeTrust.toBuffer()],
      program.programId,
    );

    await program.methods
      .createMint(9)
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        mintAuthority: mintAuthorityPda,
        mint: mintPda,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // module_state.mint should now be the mint PDA
    const state = await program.account.tokenModuleState.fetch(moduleStatePda);
    expect(state.mint.toBase58()).to.eq(mintPda.toBase58());

    // The mint account exists on-chain — verify by fetching its lamports
    const mintInfo = await provider.connection.getAccountInfo(mintPda);
    expect(mintInfo).to.not.be.null;
    expect(mintInfo!.owner.toBase58()).to.eq(TOKEN_2022_PROGRAM_ID.toBase58());
  });

  it("mint_tokens issues 1000 tokens to a recipient ATA", async () => {
    // Spawn fresh trust + init + create_mint inline for isolation
    const fakeTrust = Keypair.generate().publicKey;
    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_module"), fakeTrust.toBuffer()],
      program.programId,
    );
    const [mintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_authority"), fakeTrust.toBuffer()],
      program.programId,
    );
    const [mintPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("mint"), fakeTrust.toBuffer()],
      program.programId,
    );

    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .createMint(9)
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        mintAuthority: mintAuthorityPda,
        mint: mintPda,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Create the recipient's ATA (Token-2022 ATA is derived from the
    // Token-2022 program ID, not the legacy SPL Token program ID).
    const recipient = provider.wallet.publicKey;
    const ata = getAssociatedTokenAddressSync(
      mintPda,
      recipient,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );

    const ataIx = createAssociatedTokenAccountInstruction(
      provider.wallet.publicKey,
      ata,
      recipient,
      mintPda,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const ataTx = new anchor.web3.Transaction().add(ataIx);
    await provider.sendAndConfirm(ataTx);

    // Mint 1000 tokens
    await program.methods
      .mintTokens(new anchor.BN(1000))
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        mintAuthority: mintAuthorityPda,
        mint: mintPda,
        recipientTa: ata,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    // Verify balance
    const acct = await getAccount(
      provider.connection,
      ata,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );
    expect(acct.amount.toString()).to.eq("1000");
    expect(acct.mint.toBase58()).to.eq(mintPda.toBase58());
    expect(acct.owner.toBase58()).to.eq(recipient.toBase58());
  });

  it("finalize transitions Initialized → Finalized", async () => {
    const fakeTrust = Keypair.generate().publicKey;

    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_module"), fakeTrust.toBuffer()],
      program.programId,
    );

    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .finalize()
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
      })
      .rpc();

    const state = await program.account.tokenModuleState.fetch(moduleStatePda);
    expect(state.initialized).to.eq(2); // Finalized
  });
});
