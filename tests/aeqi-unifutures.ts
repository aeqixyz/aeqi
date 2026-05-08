import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiUnifutures } from "../target/types/aeqi_unifutures";
import { PublicKey, Keypair } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_unifutures", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiUnifutures as Program<AeqiUnifutures>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;

  // PRECISION = 1e18
  const PRECISION = new anchor.BN("1000000000000000000");

  before(() => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("unifutures_module"), fakeTrust.toBuffer()],
      program.programId,
    );
  });

  it("init creates the unifutures module state", async () => {
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.unifuturesModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.curveCount.toString()).to.eq("0");
  });

  it("create_curve stores a BondingCurve PDA (linear, 1e18→2e18, max 1000)", async () => {
    const curveId = new Uint8Array(32);
    curveId[0] = 0xb1;

    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );

    await program.methods
      .createCurve(
        Array.from(curveId),
        0, // linear
        PRECISION, // start_price = 1e18
        PRECISION.mul(new anchor.BN(2)), // end_price = 2e18
        new anchor.BN(1000), // max_supply
        900_000, // reserve_ratio = 90%
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        curve: curvePda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const c = await program.account.bondingCurve.fetch(curvePda);
    expect(c.curveType).to.eq(0);
    expect(c.startPrice.toString()).to.eq(PRECISION.toString());
    expect(c.endPrice.toString()).to.eq(PRECISION.mul(new anchor.BN(2)).toString());
    expect(c.maxSupply.toString()).to.eq("1000");
    expect(c.currentSupply.toString()).to.eq("0");
    expect(c.reserveRatioPpm).to.eq(900_000);
    expect(c.creator.toBase58()).to.eq(provider.wallet.publicKey.toBase58());

    const m = await program.account.unifuturesModuleState.fetch(modulePda);
    expect(m.curveCount.toString()).to.eq("1");
  });

  it("rejects create_curve with zero max_supply", async () => {
    const curveId = new Uint8Array(32);
    curveId[0] = 0xee;

    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .createCurve(
          Array.from(curveId),
          0,
          PRECISION,
          PRECISION.mul(new anchor.BN(2)),
          new anchor.BN(0), // ZERO max_supply
          900_000,
        )
        .accounts({
          trust: fakeTrust,
          moduleState: modulePda,
          curve: curvePda,
          creator: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/ZeroMaxSupply/);
    }
    expect(threw).to.eq(true);
  });

  it("rejects create_curve with reserve_ratio > 100%", async () => {
    const curveId = new Uint8Array(32);
    curveId[0] = 0xed;

    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .createCurve(
          Array.from(curveId),
          0,
          PRECISION,
          PRECISION.mul(new anchor.BN(2)),
          new anchor.BN(1000),
          1_500_000, // > 1_000_000 (100%)
        )
        .accounts({
          trust: fakeTrust,
          moduleState: modulePda,
          curve: curvePda,
          creator: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/InvalidReserveRatio/);
    }
    expect(threw).to.eq(true);
  });
});
