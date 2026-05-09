import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiTrust } from "../target/types/aeqi_trust";
import { PublicKey, Keypair } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_trust", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiTrust as Program<AeqiTrust>;
  const trustId = new Uint8Array(32).fill(0);
  trustId[0] = 1; // distinguish from default

  let trustPda: PublicKey;
  let trustBump: number;
  let modulePda: PublicKey;

  before(() => {
    [trustPda, trustBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      program.programId,
    );
  });

  it("initializes a trust in creation mode", async () => {
    await program.methods
      .initialize(Array.from(trustId))
      .accounts({
        trust: trustPda,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const trust = await program.account.trust.fetch(trustPda);
    expect(trust.creationMode).to.eq(true);
    expect(trust.paused).to.eq(false);
    expect(trust.moduleCount).to.eq(0);
    expect(trust.authority.toBase58()).to.eq(
      provider.wallet.publicKey.toBase58(),
    );
  });

  it("registers a module while in creation mode", async () => {
    const moduleId = new Uint8Array(32).fill(0);
    moduleId[0] = 0x52; // 'R' for role

    const dummyProgram = Keypair.generate().publicKey;

    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(moduleId)],
      program.programId,
    );

    await program.methods
      .registerModule(
        Array.from(moduleId),
        dummyProgram,
        new anchor.BN(0xff), // grant the lower 8 ACL flags
      )
      .accounts({
        trust: trustPda,
        module: modulePda,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const moduleAcct = await program.account.module.fetch(modulePda);
    expect(moduleAcct.programId.toBase58()).to.eq(dummyProgram.toBase58());
    expect(moduleAcct.trustAcl.toString()).to.eq("255");
    expect(moduleAcct.initialized).to.eq(0); // Pending
  });

  it("finalizes the trust (exits creation mode)", async () => {
    await program.methods
      .finalize()
      .accounts({
        trust: trustPda,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    const trust = await program.account.trust.fetch(trustPda);
    expect(trust.creationMode).to.eq(false);
  });

  it("rejects register_module after finalize", async () => {
    const moduleId = new Uint8Array(32).fill(0);
    moduleId[0] = 0x47; // 'G' for governance

    const [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(moduleId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .registerModule(
          Array.from(moduleId),
          Keypair.generate().publicKey,
          new anchor.BN(0),
        )
        .accounts({
          trust: trustPda,
          module: modulePda,
          authority: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/NotInCreationMode/);
    }
    expect(threw).to.eq(true);
  });

  it("rejects post-finalize config writes from non-authority even with a high-ACL module account", async () => {
    const attacker = Keypair.generate();
    const sig = await provider.connection.requestAirdrop(
      attacker.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL,
    );
    const latest = await provider.connection.getLatestBlockhash();
    await provider.connection.confirmTransaction(
      { signature: sig, ...latest },
      "confirmed",
    );

    const key = new Uint8Array(32).fill(0);
    key[0] = 0x99;
    const [configPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("cfg_num"), trustPda.toBuffer(), Buffer.from(key)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .setNumericConfig(Array.from(key), new anchor.BN(42))
        .accounts({
          trust: trustPda,
          config: configPda,
          sourceModule: modulePda,
          authority: attacker.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([attacker])
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/Unauthorized/);
    }
    expect(threw).to.eq(true);
  });
});
