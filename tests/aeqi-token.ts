import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiToken } from "../target/types/aeqi_token";
import { PublicKey, Keypair } from "@solana/web3.js";
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
    const TOKEN_2022_PROGRAM_ID = new PublicKey(
      "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
    );

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
