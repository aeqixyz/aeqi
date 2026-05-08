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

  it("sell_to_curve mirrors buy with 90% reserve_ratio applied", async () => {
    // Linear 1e18→2e18, max 1000, 90% reserve. Buy 100 (cost=105), then
    // sell 50:
    //   p(100)=1.1e18, p(50)=1.05e18, avg=1.075e18
    //   gross = 50*1.075e18/1e18 = 53 (truncated)
    //   return = 53 * 900_000 / 1_000_000 = 47
    const curveId = new Uint8Array(32);
    curveId[0] = 0x55; // distinct from buy test
    curveId[1] = 0x55;

    const [curvePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );
    const [curveAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("curve_authority"), fakeTrust.toBuffer(), Buffer.from(curveId)],
      program.programId,
    );

    await program.methods
      .createCurve(
        Array.from(curveId),
        0,
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

    // Two mints
    const assetMint = await createMint(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      provider.wallet.publicKey,
      null,
      0,
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

    // Curve vaults
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
    await provider.sendAndConfirm(
      new anchor.web3.Transaction()
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
        ),
    );
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

    // Trader ATAs + 200 quote balance
    const trader = provider.wallet.publicKey;
    const traderAssetTa = getAssociatedTokenAddressSync(
      assetMint,
      trader,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const traderQuoteTa = getAssociatedTokenAddressSync(
      quoteMint,
      trader,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(
      new anchor.web3.Transaction()
        .add(
          createAssociatedTokenAccountInstruction(
            trader,
            traderAssetTa,
            trader,
            assetMint,
            TOKEN_2022_PROGRAM_ID,
            ASSOCIATED_TOKEN_PROGRAM_ID,
          ),
        )
        .add(
          createAssociatedTokenAccountInstruction(
            trader,
            traderQuoteTa,
            trader,
            quoteMint,
            TOKEN_2022_PROGRAM_ID,
            ASSOCIATED_TOKEN_PROGRAM_ID,
          ),
        ),
    );
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      quoteMint,
      traderQuoteTa,
      trader,
      200,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Buy 100 (cost = 105) — sets supply=100, reserve=105
    await program.methods
      .buyFromCurve(new anchor.BN(100), new anchor.BN(120))
      .accounts({
        curve: curvePda,
        curveAuthority: curveAuthorityPda,
        assetMint,
        quoteMint,
        curveAssetVault,
        curveQuoteVault,
        buyerAssetTa: traderAssetTa,
        buyerQuoteTa: traderQuoteTa,
        buyer: trader,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    // Now sell 50 back. Expected return = 47.
    await program.methods
      .sellToCurve(new anchor.BN(50), new anchor.BN(40)) // min_return = 40
      .accounts({
        curve: curvePda,
        curveAuthority: curveAuthorityPda,
        assetMint,
        quoteMint,
        curveAssetVault,
        curveQuoteVault,
        sellerAssetTa: traderAssetTa,
        sellerQuoteTa: traderQuoteTa,
        seller: trader,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    // Verify:
    //   trader asset = 100 - 50 = 50
    //   trader quote = (200 - 105) + 47 = 142
    //   curve asset vault = 900 + 50 = 950
    //   curve quote vault = 105 - 47 = 58
    //   curve.current_supply = 50
    //   curve.reserve_balance = 105 - 47 = 58
    const traderAsset = await getAccount(provider.connection, traderAssetTa, undefined, TOKEN_2022_PROGRAM_ID);
    expect(traderAsset.amount.toString()).to.eq("50");

    const traderQuote = await getAccount(provider.connection, traderQuoteTa, undefined, TOKEN_2022_PROGRAM_ID);
    expect(traderQuote.amount.toString()).to.eq("142");

    const vaultAsset = await getAccount(provider.connection, curveAssetVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vaultAsset.amount.toString()).to.eq("950");

    const vaultQuote = await getAccount(provider.connection, curveQuoteVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vaultQuote.amount.toString()).to.eq("58");

    const c = await program.account.bondingCurve.fetch(curvePda);
    expect(c.currentSupply.toString()).to.eq("50");
    expect(c.reserveBalance.toString()).to.eq("58");
  });

  it("create_commitment_sale stores a fixed-price pre-sale PDA", async () => {
    const saleId = new Uint8Array(32);
    saleId[0] = 0xc5;
    saleId[1] = 0xa1;

    const [salePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sale"), fakeTrust.toBuffer(), Buffer.from(saleId)],
      program.programId,
    );

    const ASSET_AMOUNT = 1000;
    const TARGET = 5000;
    const OVERFLOW = 7500;
    const DURATION = 60 * 60 * 24 * 7; // 7 days

    await program.methods
      .createCommitmentSale(
        Array.from(saleId),
        new anchor.BN(ASSET_AMOUNT),
        new anchor.BN(TARGET),
        new anchor.BN(OVERFLOW),
        new anchor.BN(DURATION),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        sale: salePda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const s = await program.account.commitmentSale.fetch(salePda);
    expect(s.assetAmount.toString()).to.eq(String(ASSET_AMOUNT));
    expect(s.targetQuote.toString()).to.eq(String(TARGET));
    expect(s.overflowQuote.toString()).to.eq(String(OVERFLOW));
    expect(s.status).to.eq(0); // Active
    expect(s.proceedsCollected.toString()).to.eq("0");
    expect(s.creator.toBase58()).to.eq(provider.wallet.publicKey.toBase58());
    // end_time = start + DURATION; allow ±5s slack
    const expectedEnd = s.startTime.add(new anchor.BN(DURATION));
    expect(s.endTime.toString()).to.eq(expectedEnd.toString());
  });

  it("rejects create_commitment_sale when overflow < target", async () => {
    const saleId = new Uint8Array(32);
    saleId[0] = 0xee;
    saleId[1] = 0xee;

    const [salePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sale"), fakeTrust.toBuffer(), Buffer.from(saleId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .createCommitmentSale(
          Array.from(saleId),
          new anchor.BN(1000),
          new anchor.BN(5000), // target
          new anchor.BN(3000), // overflow < target — INVALID
          new anchor.BN(60 * 60 * 24),
        )
        .accounts({
          trust: fakeTrust,
          moduleState: modulePda,
          sale: salePda,
          creator: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/InvalidOverflowTarget/);
    }
    expect(threw).to.eq(true);
  });

  it("create_exit stores a pro-rata redemption Exit PDA", async () => {
    const exitId = new Uint8Array(32);
    exitId[0] = 0xe1;
    exitId[1] = 0xee;

    const [exitPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("exit"), fakeTrust.toBuffer(), Buffer.from(exitId)],
      program.programId,
    );

    const EXIT_QUOTE = 100_000;
    const SUPPLY_SNAPSHOT = 50_000;
    const DURATION = 60 * 60 * 24 * 30; // 30 days

    await program.methods
      .createExit(
        Array.from(exitId),
        new anchor.BN(EXIT_QUOTE),
        new anchor.BN(SUPPLY_SNAPSHOT),
        new anchor.BN(DURATION),
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        exit: exitPda,
        creator: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const e = await program.account.exit.fetch(exitPda);
    expect(e.exitQuote.toString()).to.eq(String(EXIT_QUOTE));
    expect(e.totalSupplySnapshot.toString()).to.eq(String(SUPPLY_SNAPSHOT));
    expect(e.remainingProceeds.toString()).to.eq(String(EXIT_QUOTE));
    expect(e.proceedsCollected.toString()).to.eq("0");
    expect(e.status).to.eq(0); // Active
    expect(e.creator.toBase58()).to.eq(provider.wallet.publicKey.toBase58());
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
