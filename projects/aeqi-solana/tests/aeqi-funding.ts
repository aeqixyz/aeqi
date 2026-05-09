import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiFunding } from "../target/types/aeqi_funding";
import { AeqiUnifutures } from "../target/types/aeqi_unifutures";
import { PublicKey, Keypair } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_funding", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiFunding as Program<AeqiFunding>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;

  before(() => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_module"), fakeTrust.toBuffer()],
      program.programId,
    );
  });

  it("init creates the funding module state", async () => {
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.fundingModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.requestCount.toString()).to.eq("0");
  });

  it("create_funding_request stores a Pending FundingRequest PDA", async () => {
    const requestId = new Uint8Array(32);
    requestId[0] = 0x52; // 'R'

    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xb1;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        0, // CommitmentSale
        Array.from(budgetId),
        new anchor.BN(10_000), // asset_amount
        new anchor.BN(50_000), // target_quote
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.kind).to.eq(0);
    expect(r.assetAmount.toString()).to.eq("10000");
    expect(r.targetQuote.toString()).to.eq("50000");
    expect(r.status).to.eq(0); // Pending
    expect(r.creator.toBase58()).to.eq(provider.wallet.publicKey.toBase58());
  });

  it("cancel_funding_request transitions Pending → Cancelled", async () => {
    const requestId = new Uint8Array(32);
    requestId[0] = 0x53;

    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xb2;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        1, // BondingCurve
        Array.from(budgetId),
        new anchor.BN(5_000),
        new anchor.BN(20_000),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .cancelFundingRequest()
      .accounts({
        request: requestPda,
        creator: provider.wallet.publicKey,
      })
      .rpc();

    const r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.status).to.eq(3); // Cancelled
  });

  it("activate_commitment_sale CPIs into aeqi_unifutures to create the sale", async () => {
    const unifutures = anchor.workspace.aeqiUnifutures as Program<AeqiUnifutures>;

    // Init the unifutures module on the same fakeTrust so CPI has a target
    const [unifModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("unifutures_module"), fakeTrust.toBuffer()],
      unifutures.programId,
    );
    await unifutures.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: unifModulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Create a Pending funding request, kind=0 (CommitmentSale)
    const requestId = new Uint8Array(32);
    requestId[0] = 0xa0;
    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xb0;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        0, // CommitmentSale
        Array.from(budgetId),
        new anchor.BN(1000),
        new anchor.BN(5000),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Now activate — this CPIs into aeqi_unifutures.create_commitment_sale
    const saleId = new Uint8Array(32);
    saleId[0] = 0xa0;
    saleId[1] = 0xa1;
    const [salePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sale"), fakeTrust.toBuffer(), Buffer.from(saleId)],
      unifutures.programId,
    );

    await program.methods
      .activateCommitmentSale(
        Array.from(saleId),
        new anchor.BN(7500), // overflow_quote
        new anchor.BN(60 * 60 * 24 * 7),
      )
      .accounts({
        request: requestPda,
        trust: fakeTrust,
        unifuturesModuleState: unifModulePda,
        sale: salePda,
        creator: provider.wallet.publicKey,
        aeqiUnifuturesProgram: unifutures.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Request now Activated, primitive_id = saleId
    const r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.status).to.eq(1); // Activated
    expect(Buffer.from(r.primitiveId).toString("hex")).to.eq(
      Buffer.from(saleId).toString("hex"),
    );

    // The CommitmentSale was actually created on aeqi_unifutures' side
    const sale = await unifutures.account.commitmentSale.fetch(salePda);
    expect(sale.assetAmount.toString()).to.eq("1000");
    expect(sale.targetQuote.toString()).to.eq("5000");
    expect(sale.overflowQuote.toString()).to.eq("7500");
    expect(sale.status).to.eq(0); // Active
  });

  it("activate_bonding_curve CPIs into aeqi_unifutures.create_curve", async () => {
    const unifutures = anchor.workspace.aeqiUnifutures as Program<AeqiUnifutures>;

    // Reuse same fakeTrust + module init from prior test (it's idempotent —
    // skip the init since it's already done above; the module_state PDA is
    // already initialized so a second init would fail).
    const [unifModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("unifutures_module"), fakeTrust.toBuffer()],
      unifutures.programId,
    );

    const requestId = new Uint8Array(32);
    requestId[0] = 0xb0;
    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xb1;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        1, // BondingCurve
        Array.from(budgetId),
        new anchor.BN(0),
        new anchor.BN(0),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const curveId = new Uint8Array(32);
    curveId[0] = 0xc0;
    curveId[1] = 0xff;
    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      unifutures.programId,
    );

    await program.methods
      .activateBondingCurve(
        Array.from(curveId),
        0, // Linear
        new anchor.BN("1000000000000000000"), // start_price 1e18
        new anchor.BN("2000000000000000000"), // end_price 2e18
        new anchor.BN(1_000_000), // max_supply
        500_000, // 50% reserve_ratio_ppm
      )
      .accounts({
        request: requestPda,
        trust: fakeTrust,
        unifuturesModuleState: unifModulePda,
        curve: curvePda,
        creator: provider.wallet.publicKey,
        aeqiUnifuturesProgram: unifutures.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.status).to.eq(1);
    expect(Buffer.from(r.primitiveId).toString("hex")).to.eq(
      Buffer.from(curveId).toString("hex"),
    );

    const c = await unifutures.account.bondingCurve.fetch(curvePda);
    expect(c.maxSupply.toString()).to.eq("1000000");
    expect(c.curveType).to.eq(0);
    expect(c.reserveRatioPpm).to.eq(500_000);
  });

  it("activate_exit CPIs into aeqi_unifutures.create_exit", async () => {
    const unifutures = anchor.workspace.aeqiUnifutures as Program<AeqiUnifutures>;

    const [unifModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("unifutures_module"), fakeTrust.toBuffer()],
      unifutures.programId,
    );

    const requestId = new Uint8Array(32);
    requestId[0] = 0xe0;
    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xe1;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        2, // Exit
        Array.from(budgetId),
        new anchor.BN(0),
        new anchor.BN(0),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const exitId = new Uint8Array(32);
    exitId[0] = 0xe0;
    exitId[1] = 0x17;
    const [exitPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("exit"), fakeTrust.toBuffer(), Buffer.from(exitId)],
      unifutures.programId,
    );

    await program.methods
      .activateExit(
        Array.from(exitId),
        new anchor.BN(100_000), // exit_quote
        new anchor.BN(500_000), // total_supply_snapshot
        new anchor.BN(60 * 60 * 24 * 30), // 30 days
      )
      .accounts({
        request: requestPda,
        trust: fakeTrust,
        unifuturesModuleState: unifModulePda,
        exit: exitPda,
        creator: provider.wallet.publicKey,
        aeqiUnifuturesProgram: unifutures.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.status).to.eq(1);
    expect(Buffer.from(r.primitiveId).toString("hex")).to.eq(
      Buffer.from(exitId).toString("hex"),
    );

    const e = await unifutures.account.exit.fetch(exitPda);
    expect(e.exitQuote.toString()).to.eq("100000");
    expect(e.totalSupplySnapshot.toString()).to.eq("500000");
    expect(e.status).to.eq(0); // Active
  });

  it("activate_bonding_curve rejects wrong-kind request", async () => {
    // Reuse the CommitmentSale request from earlier in the suite — it has
    // status=Activated already, but more importantly kind=0. Even if we
    // create a fresh Pending CommitmentSale here, kind=0 ≠ 1 → WrongKind.
    const requestId = new Uint8Array(32);
    requestId[0] = 0x77;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        0, // CommitmentSale (NOT BondingCurve)
        Array.from(new Uint8Array(32)),
        new anchor.BN(1),
        new anchor.BN(1),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const unifutures = anchor.workspace.aeqiUnifutures as Program<AeqiUnifutures>;
    const [unifModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("unifutures_module"), fakeTrust.toBuffer()],
      unifutures.programId,
    );
    const curveId = new Uint8Array(32);
    curveId[0] = 0x77;
    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      unifutures.programId,
    );

    let threw = false;
    try {
      await program.methods
        .activateBondingCurve(
          Array.from(curveId),
          0,
          new anchor.BN("1000000000000000000"),
          new anchor.BN("2000000000000000000"),
          new anchor.BN(1000),
          500_000,
        )
        .accounts({
          request: requestPda,
          trust: fakeTrust,
          unifuturesModuleState: unifModulePda,
          curve: curvePda,
          creator: provider.wallet.publicKey,
          aeqiUnifuturesProgram: unifutures.programId,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/WrongKind/);
    }
    expect(threw).to.eq(true);
  });

  it("finalize_funding_request closes the lifecycle (Activated → Finalized)", async () => {
    const unifutures = anchor.workspace.aeqiUnifutures as Program<AeqiUnifutures>;
    const [unifModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("unifutures_module"), fakeTrust.toBuffer()],
      unifutures.programId,
    );

    // Pending → activate → finalize for an Exit-kind request.
    const requestId = new Uint8Array(32);
    requestId[0] = 0xf0;
    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xf1;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        2, // Exit
        Array.from(budgetId),
        new anchor.BN(0),
        new anchor.BN(0),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const exitId = new Uint8Array(32);
    exitId[0] = 0xf2;
    const [exitPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("exit"), fakeTrust.toBuffer(), Buffer.from(exitId)],
      unifutures.programId,
    );

    await program.methods
      .activateExit(
        Array.from(exitId),
        new anchor.BN(50_000),
        new anchor.BN(200_000),
        new anchor.BN(60 * 60 * 24),
      )
      .accounts({
        request: requestPda,
        trust: fakeTrust,
        unifuturesModuleState: unifModulePda,
        exit: exitPda,
        creator: provider.wallet.publicKey,
        aeqiUnifuturesProgram: unifutures.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    let r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.status).to.eq(1); // Activated

    await program.methods
      .finalizeFundingRequest()
      .accounts({
        request: requestPda,
        creator: provider.wallet.publicKey,
      })
      .rpc();

    r = await program.account.fundingRequest.fetch(requestPda);
    expect(r.status).to.eq(2); // Finalized
  });

  it("finalize_funding_request rejects Pending requests", async () => {
    const requestId = new Uint8Array(32);
    requestId[0] = 0xf3;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    await program.methods
      .createFundingRequest(
        Array.from(requestId),
        2, // Exit (Pending)
        Array.from(new Uint8Array(32)),
        new anchor.BN(0),
        new anchor.BN(0),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        request: requestPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    let threw = false;
    try {
      await program.methods
        .finalizeFundingRequest()
        .accounts({
          request: requestPda,
          creator: provider.wallet.publicKey,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/CannotFinalize/);
    }
    expect(threw).to.eq(true);
  });

  it("rejects invalid kind (>=3)", async () => {
    const requestId = new Uint8Array(32);
    requestId[0] = 0xee;

    const [requestPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("funding_request"), fakeTrust.toBuffer(), Buffer.from(requestId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .createFundingRequest(
          Array.from(requestId),
          5, // INVALID
          Array.from(new Uint8Array(32)),
          new anchor.BN(100),
          new anchor.BN(100),
        )
        .accounts({
          trust: fakeTrust,
          moduleState: modulePda,
          request: requestPda,
          creator: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/InvalidKind/);
    }
    expect(threw).to.eq(true);
  });
});
