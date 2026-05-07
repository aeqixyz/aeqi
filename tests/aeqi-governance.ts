import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiGovernance } from "../target/types/aeqi_governance";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountInstruction,
  mintTo,
} from "@solana/spl-token";
import { expect } from "chai";

describe("aeqi_governance", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiGovernance as Program<AeqiGovernance>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;

  before(() => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_module"), fakeTrust.toBuffer()],
      program.programId,
    );
  });

  it("init creates governance module state", async () => {
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.governanceModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.proposalCount.toString()).to.eq("0");
    expect(m.configCount).to.eq(0);
  });

  it("registers a token-voting governance config", async () => {
    const tokenConfigId = new Uint8Array(32); // [0; 32] = token mode

    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), fakeTrust.toBuffer(), Buffer.from(tokenConfigId)],
      program.programId,
    );

    await program.methods
      .registerConfig(Array.from(tokenConfigId), {
        proposalThreshold: new anchor.BN(0),
        quorumBps: 4000,
        supportBps: 5000,
        votingPeriod: new anchor.BN(60 * 60 * 24 * 5), // 5 days
        executionDelay: new anchor.BN(0),
        allowEarlyEnact: false,
      })
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        governanceConfig: cfgPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const cfg = await program.account.governanceConfig.fetch(cfgPda);
    expect(cfg.quorumBps).to.eq(4000);
    expect(cfg.supportBps).to.eq(5000);
    expect(cfg.votingPeriod.toString()).to.eq("432000");
  });

  it("propose creates a Proposal PDA bound to the config", async () => {
    const tokenConfigId = new Uint8Array(32);
    const proposalId = new Uint8Array(32);
    proposalId[0] = 0xab;

    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), fakeTrust.toBuffer(), Buffer.from(tokenConfigId)],
      program.programId,
    );
    const [proposalPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), fakeTrust.toBuffer(), Buffer.from(proposalId)],
      program.programId,
    );

    const ipfsCid = new Uint8Array(64).fill(0x71); // 'q'

    await program.methods
      .propose(
        Array.from(proposalId),
        Array.from(tokenConfigId),
        Array.from(ipfsCid),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        governanceConfig: cfgPda,
        proposal: proposalPda,
        proposer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const p = await program.account.proposal.fetch(proposalPda);
    expect(p.proposer.toBase58()).to.eq(provider.wallet.publicKey.toBase58());
    expect(Buffer.from(p.proposalId).toString("hex")).to.eq(
      Buffer.from(proposalId).toString("hex"),
    );
    expect(p.executed).to.eq(false);
    expect(p.canceled).to.eq(false);
    expect(p.forVotes.toString()).to.eq("0");

    const m = await program.account.governanceModuleState.fetch(modulePda);
    expect(m.proposalCount.toString()).to.eq("1");
  });

  it("cast_vote tallies a For vote and creates VoteRecord", async () => {
    const tokenConfigId = new Uint8Array(32);
    const proposalId = new Uint8Array(32);
    proposalId[0] = 0xab; // same proposal as previous test

    const [proposalPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), fakeTrust.toBuffer(), Buffer.from(proposalId)],
      program.programId,
    );
    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        fakeTrust.toBuffer(),
        Buffer.from(proposalId),
        provider.wallet.publicKey.toBuffer(),
      ],
      program.programId,
    );

    await program.methods
      .castVote(1, new anchor.BN(1000)) // 1 = For, weight 1000
      .accounts({
        proposal: proposalPda,
        vote: votePda,
        voter: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const p = await program.account.proposal.fetch(proposalPda);
    expect(p.forVotes.toString()).to.eq("1000");
    expect(p.againstVotes.toString()).to.eq("0");
    expect(p.abstainVotes.toString()).to.eq("0");

    const v = await program.account.voteRecord.fetch(votePda);
    expect(v.choice).to.eq(1);
    expect(v.weight.toString()).to.eq("1000");
    expect(v.voter.toBase58()).to.eq(provider.wallet.publicKey.toBase58());
  });

  it("rejects double-voting via PDA uniqueness", async () => {
    const proposalId = new Uint8Array(32);
    proposalId[0] = 0xab;

    const [proposalPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), fakeTrust.toBuffer(), Buffer.from(proposalId)],
      program.programId,
    );
    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        fakeTrust.toBuffer(),
        Buffer.from(proposalId),
        provider.wallet.publicKey.toBuffer(),
      ],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .castVote(0, new anchor.BN(500))
        .accounts({
          proposal: proposalPda,
          vote: votePda,
          voter: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      // VoteRecord PDA already exists — init will fail
      expect(e.toString()).to.match(/already in use|custom program error/);
    }
    expect(threw).to.eq(true);
  });

  it("execute_proposal advances state when quorum + support met (early enact)", async () => {
    // Fresh config that allows early enact, fresh proposal, single For vote.
    const cfgId = new Uint8Array(32);
    cfgId[0] = 0xee; // 'e' for early-enact config

    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), fakeTrust.toBuffer(), Buffer.from(cfgId)],
      program.programId,
    );

    await program.methods
      .registerConfig(Array.from(cfgId), {
        proposalThreshold: new anchor.BN(0),
        quorumBps: 4000,
        supportBps: 5000,
        votingPeriod: new anchor.BN(60), // 1 minute
        executionDelay: new anchor.BN(0),
        allowEarlyEnact: true,
      })
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        governanceConfig: cfgPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const propId = new Uint8Array(32);
    propId[0] = 0xee;
    propId[1] = 0xee;

    const [propPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), fakeTrust.toBuffer(), Buffer.from(propId)],
      program.programId,
    );

    await program.methods
      .propose(Array.from(propId), Array.from(cfgId), Array.from(new Uint8Array(64)))
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        governanceConfig: cfgPda,
        proposal: propPda,
        proposer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        fakeTrust.toBuffer(),
        Buffer.from(propId),
        provider.wallet.publicKey.toBuffer(),
      ],
      program.programId,
    );

    await program.methods
      .castVote(1, new anchor.BN(1000)) // For, 1000 weight
      .accounts({
        proposal: propPda,
        vote: votePda,
        voter: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Total vote supply = 1000. Quorum: 40% = 400. We have 1000 participating.
    // Support: 100% For of 1000 decisive. Both thresholds met.
    await program.methods
      .executeProposal(new anchor.BN(1000))
      .accounts({
        proposal: propPda,
        governanceConfig: cfgPda,
        executor: provider.wallet.publicKey,
      })
      .rpc();

    const p = await program.account.proposal.fetch(propPda);
    expect(p.executed).to.eq(true);
    expect(p.succeededAt.toString()).to.not.eq("0");
  });

  it("execute_proposal rejects when quorum not met", async () => {
    const cfgId = new Uint8Array(32);
    cfgId[0] = 0xed;

    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), fakeTrust.toBuffer(), Buffer.from(cfgId)],
      program.programId,
    );

    await program.methods
      .registerConfig(Array.from(cfgId), {
        proposalThreshold: new anchor.BN(0),
        quorumBps: 4000,
        supportBps: 5000,
        votingPeriod: new anchor.BN(60),
        executionDelay: new anchor.BN(0),
        allowEarlyEnact: true,
      })
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        governanceConfig: cfgPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const propId = new Uint8Array(32);
    propId[0] = 0xed;
    propId[1] = 0xed;

    const [propPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), fakeTrust.toBuffer(), Buffer.from(propId)],
      program.programId,
    );

    await program.methods
      .propose(Array.from(propId), Array.from(cfgId), Array.from(new Uint8Array(64)))
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        governanceConfig: cfgPda,
        proposal: propPda,
        proposer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        fakeTrust.toBuffer(),
        Buffer.from(propId),
        provider.wallet.publicKey.toBuffer(),
      ],
      program.programId,
    );

    await program.methods
      .castVote(1, new anchor.BN(100)) // tiny weight
      .accounts({
        proposal: propPda,
        vote: votePda,
        voter: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // total_vote_supply = 1_000_000 → 40% quorum = 400_000. 100 participating ≪ 400_000.
    let threw = false;
    try {
      await program.methods
        .executeProposal(new anchor.BN(1_000_000))
        .accounts({
          proposal: propPda,
          governanceConfig: cfgPda,
          executor: provider.wallet.publicKey,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/QuorumNotMet/);
    }
    expect(threw).to.eq(true);
  });

  it("cast_vote_token reads weight from real Token-2022 balance", async () => {
    // Fresh trust + governance config + proposal, plus a real Token-2022
    // mint with the voter holding 1500 tokens. cast_vote_token should
    // record weight = 1500 from the on-chain balance, not a passed param.

    const trustV = Keypair.generate().publicKey;

    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_module"), trustV.toBuffer()],
      program.programId,
    );
    await program.methods
      .init()
      .accounts({
        trust: trustV,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const tokenCfgId = new Uint8Array(32); // [0;32] = token mode
    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), trustV.toBuffer(), Buffer.from(tokenCfgId)],
      program.programId,
    );
    await program.methods
      .registerConfig(Array.from(tokenCfgId), {
        proposalThreshold: new anchor.BN(0),
        quorumBps: 4000,
        supportBps: 5000,
        votingPeriod: new anchor.BN(60),
        executionDelay: new anchor.BN(0),
        allowEarlyEnact: true,
      })
      .accounts({
        trust: trustV,
        moduleState: moduleStatePda,
        governanceConfig: cfgPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const proposalId = new Uint8Array(32);
    proposalId[0] = 0xb1;
    const [proposalPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("proposal"), trustV.toBuffer(), Buffer.from(proposalId)],
      program.programId,
    );
    await program.methods
      .propose(
        Array.from(proposalId),
        Array.from(tokenCfgId),
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trustV,
        moduleState: moduleStatePda,
        governanceConfig: cfgPda,
        proposal: proposalPda,
        proposer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Token-2022 mint setup
    const mint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      9,
      Keypair.generate(),
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    const voter = provider.wallet.publicKey;
    const voterAta = getAssociatedTokenAddressSync(
      mint,
      voter,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(
      new anchor.web3.Transaction().add(
        createAssociatedTokenAccountInstruction(
          voter,
          voterAta,
          voter,
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
      voterAta,
      voter,
      1500,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    const [votePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("vote"),
        trustV.toBuffer(),
        Buffer.from(proposalId),
        voter.toBuffer(),
      ],
      program.programId,
    );

    await program.methods
      .castVoteToken(1) // For
      .accounts({
        proposal: proposalPda,
        vote: votePda,
        voterTokenAccount: voterAta,
        mint,
        voter,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const p = await program.account.proposal.fetch(proposalPda);
    expect(p.forVotes.toString()).to.eq("1500");

    const v = await program.account.voteRecord.fetch(votePda);
    expect(v.weight.toString()).to.eq("1500");
    expect(v.choice).to.eq(1);
  });

  it("rejects register_config with invalid bps", async () => {
    const cfgId = new Uint8Array(32);
    cfgId[0] = 0xff; // distinct from previous tests' 0xee/0xed

    const [cfgPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_config"), fakeTrust.toBuffer(), Buffer.from(cfgId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .registerConfig(Array.from(cfgId), {
          proposalThreshold: new anchor.BN(0),
          quorumBps: 12000, // > 10000 invalid
          supportBps: 5000,
          votingPeriod: new anchor.BN(86400),
          executionDelay: new anchor.BN(0),
          allowEarlyEnact: false,
        })
        .accounts({
          trust: fakeTrust,
          moduleState: modulePda,
          governanceConfig: cfgPda,
          payer: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/InvalidBpsValue/);
    }
    expect(threw).to.eq(true);
  });
});
