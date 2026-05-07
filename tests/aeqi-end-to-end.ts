/**
 * AEIQ end-to-end spawn — the full architecture proof.
 *
 * Spawns an AEIQ TRUST via aeqi_factory.create_with_modules, then initializes
 * the role / token / governance modules under it, registers role types and a
 * governance config, runs a proposal lifecycle (propose → vote → execute) end
 * to end. Exercises every program in one tx graph.
 */
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiTrust } from "../target/types/aeqi_trust";
import { AeqiFactory } from "../target/types/aeqi_factory";
import { AeqiRole } from "../target/types/aeqi_role";
import { AeqiToken } from "../target/types/aeqi_token";
import { AeqiGovernance } from "../target/types/aeqi_governance";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("AEIQ end-to-end spawn", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const trust = anchor.workspace.aeqiTrust as Program<AeqiTrust>;
  const factory = anchor.workspace.aeqiFactory as Program<AeqiFactory>;
  const role = anchor.workspace.aeqiRole as Program<AeqiRole>;
  const token = anchor.workspace.aeqiToken as Program<AeqiToken>;
  const governance = anchor.workspace.aeqiGovernance as Program<AeqiGovernance>;

  const trustId = new Uint8Array(32);
  trustId[0] = 0x41; // 'A' for AEIQ
  trustId[1] = 0x45; // 'E'
  trustId[2] = 0x49; // 'I'
  trustId[3] = 0x51; // 'Q'

  let trustPda: PublicKey;
  let roleModuleIdBytes: Uint8Array;
  let tokenModuleIdBytes: Uint8Array;
  let govModuleIdBytes: Uint8Array;

  before(() => {
    [trustPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      trust.programId,
    );
    roleModuleIdBytes = new Uint8Array(32);
    roleModuleIdBytes[0] = 0x52; // 'R'
    tokenModuleIdBytes = new Uint8Array(32);
    tokenModuleIdBytes[0] = 0x54; // 'T'
    govModuleIdBytes = new Uint8Array(32);
    govModuleIdBytes[0] = 0x47; // 'G'
  });

  it("step 1: factory.create_with_modules spawns AEIQ trust + registers 3 modules + finalizes", async () => {
    const [roleModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(roleModuleIdBytes)],
      trust.programId,
    );
    const [tokenModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(tokenModuleIdBytes)],
      trust.programId,
    );
    const [govModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(govModuleIdBytes)],
      trust.programId,
    );

    await factory.methods
      .createWithModules(Array.from(trustId), [
        {
          moduleId: Array.from(roleModuleIdBytes),
          programId: role.programId,
          trustAcl: new anchor.BN(0xff),
        },
        {
          moduleId: Array.from(tokenModuleIdBytes),
          programId: token.programId,
          trustAcl: new anchor.BN(0xff),
        },
        {
          moduleId: Array.from(govModuleIdBytes),
          programId: governance.programId,
          trustAcl: new anchor.BN(0xff),
        },
      ])
      .accounts({
        trust: trustPda,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts([
        { pubkey: roleModulePda, isWritable: true, isSigner: false },
        { pubkey: tokenModulePda, isWritable: true, isSigner: false },
        { pubkey: govModulePda, isWritable: true, isSigner: false },
      ])
      .rpc();

    const t = await trust.account.trust.fetch(trustPda);
    expect(t.creationMode).to.eq(false);
    expect(t.moduleCount).to.eq(3);
    expect(t.authority.toBase58()).to.eq(provider.wallet.publicKey.toBase58());

    // Verify each module record was created with the right program ID
    const r = await trust.account.module.fetch(roleModulePda);
    expect(r.programId.toBase58()).to.eq(role.programId.toBase58());
    const tk = await trust.account.module.fetch(tokenModulePda);
    expect(tk.programId.toBase58()).to.eq(token.programId.toBase58());
    const g = await trust.account.module.fetch(govModulePda);
    expect(g.programId.toBase58()).to.eq(governance.programId.toBase58());
  });

  it("step 2: init each module under the AEIQ trust (role, token, governance)", async () => {
    // role.init
    const [roleModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_module"), trustPda.toBuffer()],
      role.programId,
    );
    await role.methods
      .init()
      .accounts({
        trust: trustPda,
        moduleState: roleModuleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // token.init
    const [tokenModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_module"), trustPda.toBuffer()],
      token.programId,
    );
    await token.methods
      .init()
      .accounts({
        trust: trustPda,
        moduleState: tokenModuleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // governance.init
    const [govModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_module"), trustPda.toBuffer()],
      governance.programId,
    );
    await governance.methods
      .init()
      .accounts({
        trust: trustPda,
        moduleState: govModuleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // All three module-state PDAs exist + bound to the AEIQ trust
    const rs = await role.account.roleModuleState.fetch(roleModuleStatePda);
    expect(rs.trust.toBase58()).to.eq(trustPda.toBase58());
    expect(rs.initialized).to.eq(true);

    const ts = await token.account.tokenModuleState.fetch(tokenModuleStatePda);
    expect(ts.trust.toBase58()).to.eq(trustPda.toBase58());

    const gs = await governance.account.governanceModuleState.fetch(govModuleStatePda);
    expect(gs.trust.toBase58()).to.eq(trustPda.toBase58());
  });

  it("step 3: register role types — director (h=0) + ceo (h=1)", async () => {
    const directorTypeId = new Uint8Array(32);
    directorTypeId[0] = 0x44;
    directorTypeId[1] = 0x49;
    directorTypeId[2] = 0x52;

    const ceoTypeId = new Uint8Array(32);
    ceoTypeId[0] = 0x43;
    ceoTypeId[1] = 0x45;
    ceoTypeId[2] = 0x4f;

    for (const [id, hierarchy] of [[directorTypeId, 0], [ceoTypeId, 1]] as const) {
      const [pda] = PublicKey.findProgramAddressSync(
        [Buffer.from("role_type"), trustPda.toBuffer(), Buffer.from(id)],
        role.programId,
      );

      await role.methods
        .createRoleType(Array.from(id), hierarchy as number, {
          vesting: false,
          vestingCliff: new anchor.BN(0),
          vestingDuration: new anchor.BN(0),
          fdv: false,
          fdvStart: new anchor.BN(0),
          fdvEnd: new anchor.BN(0),
          probationaryPeriod: new anchor.BN(0),
          severancePeriod: new anchor.BN(0),
          contribution: false,
        })
        .accounts({
          trust: trustPda,
          roleType: pda,
          payer: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    }
  });

  it("step 4: register governance config (token-vote) and run a proposal lifecycle", async () => {
    const [govModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_module"), trustPda.toBuffer()],
      governance.programId,
    );

    const tokenCfgId = new Uint8Array(32); // [0; 32] = token mode
    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), trustPda.toBuffer(), Buffer.from(tokenCfgId)],
      governance.programId,
    );

    await governance.methods
      .registerConfig(Array.from(tokenCfgId), {
        proposalThreshold: new anchor.BN(0),
        quorumBps: 4000,
        supportBps: 5000,
        votingPeriod: new anchor.BN(60),
        executionDelay: new anchor.BN(0),
        allowEarlyEnact: true,
      })
      .accounts({
        trust: trustPda,
        moduleState: govModuleStatePda,
        governanceConfig: cfgPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Propose
    const proposalId = new Uint8Array(32);
    proposalId[0] = 0x70; // 'p'
    proposalId[1] = 0x31; // '1'
    const [proposalPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), trustPda.toBuffer(), Buffer.from(proposalId)],
      governance.programId,
    );

    await governance.methods
      .propose(
        Array.from(proposalId),
        Array.from(tokenCfgId),
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trustPda,
        moduleState: govModuleStatePda,
        governanceConfig: cfgPda,
        proposal: proposalPda,
        proposer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Vote (For, weight 1000)
    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        trustPda.toBuffer(),
        Buffer.from(proposalId),
        provider.wallet.publicKey.toBuffer(),
      ],
      governance.programId,
    );
    await governance.methods
      .castVote(1, new anchor.BN(1000))
      .accounts({
        proposal: proposalPda,
        vote: votePda,
        voter: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Execute (early enact, 1000 vs 1000 supply → 100% participation, 100% support)
    await governance.methods
      .executeProposal(new anchor.BN(1000))
      .accounts({
        proposal: proposalPda,
        governanceConfig: cfgPda,
        executor: provider.wallet.publicKey,
      })
      .rpc();

    const p = await governance.account.proposal.fetch(proposalPda);
    expect(p.executed).to.eq(true);
    expect(p.forVotes.toString()).to.eq("1000");
  });
});
