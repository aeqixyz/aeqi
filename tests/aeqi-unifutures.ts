import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiUnifutures } from "../target/types/aeqi_unifutures";
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

  it("buy_from_curve transfers quote in + asset out, updates supply", async () => {
    // Curve: linear, 1e18 → 2e18, max 1000.
    // Buy 100 at supply=0: price(0)=1e18, price(100)=1.1e18, avg=1.05e18,
    // cost = 100 * 1.05e18 / 1e18 = 105.
    const curveId = new Uint8Array(32);
    curveId[0] = 0xcc; // distinct from earlier curves
    curveId[1] = 0xcc;

    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );
    const [curveAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve_authority"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );

    // Create the curve
    await program.methods
      .createCurve(
        Array.from(curveId),
        0, // linear
        PRECISION,
        PRECISION.mul(new anchor.BN(2)),
        new anchor.BN(1000),
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

    // Two Token-2022 mints — asset (cap-table) + quote (USDC-like)
    const assetMint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      0, // 0 decimals so amount math is integer-clean
      Keypair.generate(),
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );
    const quoteMint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      0,
      Keypair.generate(),
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Curve vaults — owned by curveAuthorityPda
    const curveAssetVault = getAssociatedTokenAddressSync(
      assetMint,
      curveAuthorityPda,
      true,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const curveQuoteVault = getAssociatedTokenAddressSync(
      quoteMint,
      curveAuthorityPda,
      true,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const tx1 = new anchor.web3.Transaction()
      .add(
        createAssociatedTokenAccountInstruction(
          provider.wallet.publicKey,
          curveAssetVault,
          curveAuthorityPda,
          assetMint,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      )
      .add(
        createAssociatedTokenAccountInstruction(
          provider.wallet.publicKey,
          curveQuoteVault,
          curveAuthorityPda,
          quoteMint,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      );
    await provider.sendAndConfirm(tx1);

    // Premine 1000 asset tokens into the curve vault
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      assetMint,
      curveAssetVault,
      provider.wallet.publicKey,
      1000,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Buyer ATAs + quote balance
    const buyer = provider.wallet.publicKey;
    const buyerAssetAta = getAssociatedTokenAddressSync(
      assetMint,
      buyer,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const buyerQuoteAta = getAssociatedTokenAddressSync(
      quoteMint,
      buyer,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const tx2 = new anchor.web3.Transaction()
      .add(
        createAssociatedTokenAccountInstruction(
          buyer,
          buyerAssetAta,
          buyer,
          assetMint,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      )
      .add(
        createAssociatedTokenAccountInstruction(
          buyer,
          buyerQuoteAta,
          buyer,
          quoteMint,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      );
    await provider.sendAndConfirm(tx2);

    // Fund buyer with 200 quote
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      quoteMint,
      buyerQuoteAta,
      buyer,
      200,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Execute the buy
    await program.methods
      .buyFromCurve(new anchor.BN(100), new anchor.BN(120)) // max_cost = 120
      .accounts({
        curve: curvePda,
        curveAuthority: curveAuthorityPda,
        assetMint,
        quoteMint,
        curveAssetVault,
        curveQuoteVault,
        buyerAssetTa: buyerAssetAta,
        buyerQuoteTa: buyerQuoteAta,
        buyer,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    // Verify:
    //   buyer asset = 100, buyer quote = 200 - 105 = 95
    //   curve asset vault = 1000 - 100 = 900, curve quote vault = 105
    //   curve.current_supply = 100, curve.reserve_balance = 105
    const buyerAsset = await getAccount(provider.connection, buyerAssetAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect(buyerAsset.amount.toString()).to.eq("100");

    const buyerQuote = await getAccount(provider.connection, buyerQuoteAta, undefined, TOKEN_2022_PROGRAM_ID);
    expect(buyerQuote.amount.toString()).to.eq("95");

    const vaultAsset = await getAccount(provider.connection, curveAssetVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vaultAsset.amount.toString()).to.eq("900");

    const vaultQuote = await getAccount(provider.connection, curveQuoteVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vaultQuote.amount.toString()).to.eq("105");

    const c = await program.account.bondingCurve.fetch(curvePda);
    expect(c.currentSupply.toString()).to.eq("100");
    expect(c.reserveBalance.toString()).to.eq("105");
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
