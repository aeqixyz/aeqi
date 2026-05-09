/**
 * AEQI end-to-end spawn — the full architecture proof.
 *
 * Spawns an AEQI TRUST via aeqi_factory.create_with_modules, then initializes
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
import { Keypair, PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("AEQI end-to-end spawn", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const trust = anchor.workspace.aeqiTrust as Program<AeqiTrust>;
  const factory = anchor.workspace.aeqiFactory as Program<AeqiFactory>;
  const role = anchor.workspace.aeqiRole as Program<AeqiRole>;
  const token = anchor.workspace.aeqiToken as Program<AeqiToken>;
  const governance = anchor.workspace.aeqiGovernance as Program<AeqiGovernance>;

  const trustId = new Uint8Array(32);
  trustId[0] = 0x41; // 'A' for AEQI
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

  it("step 1: factory.create_with_modules spawns AEQI trust + registers 3 modules + finalizes", async () => {
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

  it("step 2: init each module under the AEQI trust (role, token, governance)", async () => {
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

    // All three module-state PDAs exist + bound to the AEQI trust
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

  // The skeptic-grade walk: this stitches together pieces that exist in
  // separate tests today.
  //
  //   real director seat → cross-program role-checkpoint → role-vote → execute
  //
  // Steps 1–4 above proved trust spawn + module init + role types + a
  // governance lifecycle that took weight as a u64 argument. This one
  // instead:
  //   - creates a Director Role under the AEQI trust
  //   - assigns it to a *different* wallet (Alice) — exercises the
  //     `assign_role` checkpoint-PDA fix where the checkpoint is keyed on
  //     the assignee, not the payer
  //   - registers a per-role-type governance config (id == director_type_id)
  //   - has Alice (signing herself) propose, cast_vote_role, and execute,
  //     where weight is *read* from the cross-program RoleVoteCheckpoint
  //     PDA owned by aeqi_role and validated by aeqi_governance via
  //     `seeds::program = AEQI_ROLE_ID` + manual borsh decode.
  it("step 5: assign director Role to Alice → role-vote proposal → execute", async () => {
    const role = anchor.workspace.aeqiRole as Program<AeqiRole>;

    // Alice is a real wallet, separate from provider.wallet.
    const alice = Keypair.generate();
    const sig = await provider.connection.requestAirdrop(
      alice.publicKey,
      1_000_000_000, // 1 SOL
    );
    await provider.connection.confirmTransaction(sig);

    const directorTypeId = new Uint8Array(32);
    directorTypeId[0] = 0x44;
    directorTypeId[1] = 0x49;
    directorTypeId[2] = 0x52;

    const [rtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), trustPda.toBuffer(), Buffer.from(directorTypeId)],
      role.programId,
    );

    // Create the Director Role (no parent — this is a root seat).
    const directorRoleId = new Uint8Array(32);
    directorRoleId[0] = 0x44; // 'D'
    directorRoleId[1] = 0x52; // 'R' — Director Role
    const [rolePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), trustPda.toBuffer(), Buffer.from(directorRoleId)],
      role.programId,
    );
    await role.methods
      .createRole(
        Array.from(directorRoleId),
        Array.from(directorTypeId),
        null,
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trustPda,
        roleType: rtPda,
        role: rolePda,
        callerRole: null,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Assign that Role to Alice. Critical: provider.wallet pays, but
    // Alice is the assignee — checkpoint PDA must be keyed on Alice.
    const [aliceCkptPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("role_ckpt"),
        trustPda.toBuffer(),
        Buffer.from(directorTypeId),
        alice.publicKey.toBuffer(),
      ],
      role.programId,
    );
    await role.methods
      .assignRole(alice.publicKey)
      .accounts({
        role: rolePda,
        roleType: rtPda,
        trust: trustPda,
        checkpoint: aliceCkptPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Register a per-role-type governance config keyed at directorTypeId.
    // cast_vote_role requires `proposal.governance_config_id == ckpt.role_type_id`.
    const [govModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_module"), trustPda.toBuffer()],
      governance.programId,
    );
    const [roleCfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), trustPda.toBuffer(), Buffer.from(directorTypeId)],
      governance.programId,
    );
    await governance.methods
      .registerConfig(Array.from(directorTypeId), {
        proposalThreshold: new anchor.BN(0),
        quorumBps: 5000,
        supportBps: 5000,
        votingPeriod: new anchor.BN(60),
        executionDelay: new anchor.BN(0),
        allowEarlyEnact: true,
      })
      .accounts({
        trust: trustPda,
        moduleState: govModuleStatePda,
        governanceConfig: roleCfgPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Alice proposes — she signs the tx herself.
    const proposalId = new Uint8Array(32);
    proposalId[0] = 0x52; // 'R' — role-vote proposal
    proposalId[1] = 0x76; // 'v'
    const [proposalPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), trustPda.toBuffer(), Buffer.from(proposalId)],
      governance.programId,
    );
    await governance.methods
      .propose(
        Array.from(proposalId),
        Array.from(directorTypeId),
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trustPda,
        moduleState: govModuleStatePda,
        governanceConfig: roleCfgPda,
        proposal: proposalPda,
        proposer: alice.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([alice])
      .rpc();

    // Alice casts cast_vote_role — weight comes from her checkpoint, not
    // from a u64 argument. aeqi_governance reads aeqi_role's PDA via
    // `seeds::program = AEQI_ROLE_ID` and borsh-decodes the checkpoint.
    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        trustPda.toBuffer(),
        Buffer.from(proposalId),
        alice.publicKey.toBuffer(),
      ],
      governance.programId,
    );
    await governance.methods
      .castVoteRole(1) // For
      .accounts({
        proposal: proposalPda,
        vote: votePda,
        voterCheckpoint: aliceCkptPda,
        voter: alice.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([alice])
      .rpc();

    // Execute — total_role_supply = 1 (one director seat). 1 For-vote ÷
    // 1 supply = 100% participation, 100% support → passes 50/50 thresholds.
    await governance.methods
      .executeProposal(new anchor.BN(1))
      .accounts({
        proposal: proposalPda,
        governanceConfig: roleCfgPda,
        executor: alice.publicKey,
      })
      .signers([alice])
      .rpc();

    const p = await governance.account.proposal.fetch(proposalPda);
    expect(p.executed).to.eq(true);
    expect(p.forVotes.toString()).to.eq("1");

    // Sanity: Alice's checkpoint reflects 1 director seat held.
    const ckpt = await role.account.roleVoteCheckpoint.fetch(aliceCkptPda);
    expect(ckpt.account.toBase58()).to.eq(alice.publicKey.toBase58());
    expect(ckpt.count.toString()).to.eq("1");
    expect(Buffer.from(ckpt.roleTypeId).toString("hex")).to.eq(
      Buffer.from(directorTypeId).toString("hex"),
    );
  });
});
