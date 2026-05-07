import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiGovernance } from "../target/types/aeqi_governance";
import { PublicKey, Keypair } from "@solana/web3.js";
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

  it("rejects register_config with invalid bps", async () => {
    const cfgId = new Uint8Array(32);
    cfgId[0] = 0xee;

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
