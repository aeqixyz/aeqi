import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiFund } from "../target/types/aeqi_fund";
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

describe("aeqi_fund", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiFund as Program<AeqiFund>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;

  before(() => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund_module"), fakeTrust.toBuffer()],
      program.programId,
    );
  });

  it("init creates the fund module state", async () => {
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.fundModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.fundCount.toString()).to.eq("0");
  });

  it("create_fund + deposit + redeem — full LP cycle", async () => {
    const fundId = new Uint8Array(32);
    fundId[0] = 0xfd;
    fundId[1] = 0x01;

    const [fundPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund"), fakeTrust.toBuffer(), Buffer.from(fundId)],
      program.programId,
    );
    const [fundAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund_authority"), fakeTrust.toBuffer(), Buffer.from(fundId)],
      program.programId,
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

    await program.methods
      .createFund(Array.from(fundId), 2000) // 20% carry
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        fund: fundPda,
        quoteMint,
        manager: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const fundQuoteVault = getAssociatedTokenAddressSync(
      quoteMint,
      fundAuthorityPda,
      true,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const lp = provider.wallet.publicKey;
    const lpQuoteTa = getAssociatedTokenAddressSync(
      quoteMint,
      lp,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(
      new anchor.web3.Transaction()
        .add(
          createAssociatedTokenAccountInstruction(
            lp,
            fundQuoteVault,
            fundAuthorityPda,
            quoteMint,
            TOKEN_2022_PROGRAM_ID,
            ASSOCIATED_TOKEN_PROGRAM_ID,
          ),
        )
        .add(
          createAssociatedTokenAccountInstruction(
            lp,
            lpQuoteTa,
            lp,
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
      lpQuoteTa,
      lp,
      10_000,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    const [lpSharePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("lp_share"),
        fakeTrust.toBuffer(),
        Buffer.from(fundId),
        lp.toBuffer(),
      ],
      program.programId,
    );

    // First deposit: 5000 quote → 5000 shares (1:1)
    await program.methods
      .deposit(new anchor.BN(5000))
      .accounts({
        fund: fundPda,
        fundAuthority: fundAuthorityPda,
        quoteMint,
        fundQuoteVault,
        lpQuoteTa,
        lpShare: lpSharePda,
        lp,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    let f = await program.account.fund.fetch(fundPda);
    expect(f.grossNav.toString()).to.eq("5000");
    expect(f.totalShares.toString()).to.eq("5000");

    let s = await program.account.lpShare.fetch(lpSharePda);
    expect(s.shares.toString()).to.eq("5000");

    let vault = await getAccount(provider.connection, fundQuoteVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vault.amount.toString()).to.eq("5000");

    // Second deposit: 3000 quote → with NAV=5000, shares=5000:
    // shares = 3000 * 5000 / 5000 = 3000 (1:1 since no NAV growth yet)
    await program.methods
      .deposit(new anchor.BN(3000))
      .accounts({
        fund: fundPda,
        fundAuthority: fundAuthorityPda,
        quoteMint,
        fundQuoteVault,
        lpQuoteTa,
        lpShare: lpSharePda,
        lp,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    f = await program.account.fund.fetch(fundPda);
    expect(f.grossNav.toString()).to.eq("8000");
    expect(f.totalShares.toString()).to.eq("8000");

    s = await program.account.lpShare.fetch(lpSharePda);
    expect(s.shares.toString()).to.eq("8000");

    // Redeem 4000 shares: quote_out = 4000 * 8000 / 8000 = 4000
    await program.methods
      .redeem(new anchor.BN(4000))
      .accounts({
        fund: fundPda,
        fundAuthority: fundAuthorityPda,
        quoteMint,
        fundQuoteVault,
        lpQuoteTa,
        lpShare: lpSharePda,
        lp,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();

    f = await program.account.fund.fetch(fundPda);
    expect(f.grossNav.toString()).to.eq("4000");
    expect(f.totalShares.toString()).to.eq("4000");

    s = await program.account.lpShare.fetch(lpSharePda);
    expect(s.shares.toString()).to.eq("4000");

    vault = await getAccount(provider.connection, fundQuoteVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vault.amount.toString()).to.eq("4000");

    const lpAccount = await getAccount(provider.connection, lpQuoteTa, undefined, TOKEN_2022_PROGRAM_ID);
    // Started 10_000, deposited 8000, redeemed 4000 → 6000 in LP wallet
    expect(lpAccount.amount.toString()).to.eq("6000");
  });
});
