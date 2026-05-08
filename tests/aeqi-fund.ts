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

  // NAV-up → carry accrues at 20% of the increase, gross_nav reflects
  // post-carry LP-attributable value, HWM resets to the new gross_nav.
  // Manager then claims the accrued carry from the vault.
  it("update_nav accrues carry past HWM + claim_carry settles to manager", async () => {
    const fundId = new Uint8Array(32);
    fundId[0] = 0xfd;
    fundId[1] = 0x02;

    const [fundPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund"), fakeTrust.toBuffer(), Buffer.from(fundId)],
      program.programId,
    );
    const [fundAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund_authority"), fakeTrust.toBuffer(), Buffer.from(fundId)],
      program.programId,
    );

    // Fresh quote mint + manager wallet that's separate from the LP. Tests
    // the manager-only auth check on update_nav and claim_carry.
    const manager = Keypair.generate();
    const sig = await provider.connection.requestAirdrop(
      manager.publicKey,
      1_000_000_000,
    );
    await provider.connection.confirmTransaction(sig);

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
        manager: manager.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([manager])
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
    const managerQuoteTa = getAssociatedTokenAddressSync(
      quoteMint,
      manager.publicKey,
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
        )
        .add(
          createAssociatedTokenAccountInstruction(
            lp,
            managerQuoteTa,
            manager.publicKey,
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
      1000,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // LP deposits 1000. After: gross_nav=1000, total_shares=1000, hwm=0.
    const [lpSharePda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("lp_share"),
        fakeTrust.toBuffer(),
        Buffer.from(fundId),
        lp.toBuffer(),
      ],
      program.programId,
    );
    await program.methods
      .deposit(new anchor.BN(1000))
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

    // Simulate portfolio gain: mint 200 extra quote into the vault. The
    // manager's mark-to-market `new_gross_nav` will reflect this.
    await mintTo(
      provider.connection,
      (provider.wallet as anchor.Wallet).payer,
      quoteMint,
      fundQuoteVault,
      lp, // mint authority is provider.wallet
      200,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    // Manager reports new vault value 1200. lp_nav = 1200 - 0 = 1200,
    // crosses HWM (was 0), increase = 1200, carry = 1200 * 20% = 240.
    // accrued_carry = 240, gross_nav = 1200 - 240 = 960, hwm = 960.
    await program.methods
      .updateNav(new anchor.BN(1200))
      .accounts({
        fund: fundPda,
        manager: manager.publicKey,
      })
      .signers([manager])
      .rpc();

    let f = await program.account.fund.fetch(fundPda);
    expect(f.grossNav.toString()).to.eq("960");
    expect(f.highWaterMark.toString()).to.eq("960");
    expect(f.accruedCarry.toString()).to.eq("240");
    expect(f.totalShares.toString()).to.eq("1000"); // shares unchanged

    // Manager claims the accrued carry — vault transfers 240 → manager TA.
    await program.methods
      .claimCarry()
      .accounts({
        fund: fundPda,
        fundAuthority: fundAuthorityPda,
        quoteMint,
        fundQuoteVault,
        managerQuoteTa,
        manager: manager.publicKey,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .signers([manager])
      .rpc();

    f = await program.account.fund.fetch(fundPda);
    expect(f.accruedCarry.toString()).to.eq("0");
    expect(f.grossNav.toString()).to.eq("960"); // LP NAV unaffected by claim

    const mgrAcct = await getAccount(provider.connection, managerQuoteTa, undefined, TOKEN_2022_PROGRAM_ID);
    expect(mgrAcct.amount.toString()).to.eq("240");
    const vault = await getAccount(provider.connection, fundQuoteVault, undefined, TOKEN_2022_PROGRAM_ID);
    expect(vault.amount.toString()).to.eq("960"); // 1200 minted - 240 claimed
  });

  it("update_nav rejects calls from non-manager", async () => {
    // The fund from the carry-walk test above; non-manager (provider.wallet)
    // tries to update_nav and must hit NotManager.
    const fundId = new Uint8Array(32);
    fundId[0] = 0xfd;
    fundId[1] = 0x02;
    const [fundPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund"), fakeTrust.toBuffer(), Buffer.from(fundId)],
      program.programId,
    );

    let threw = false;
    try {
      await program.methods
        .updateNav(new anchor.BN(2000))
        .accounts({
          fund: fundPda,
          manager: provider.wallet.publicKey,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/NotManager/);
    }
    expect(threw).to.eq(true);
  });

  it("update_nav with no HWM cross does not accrue carry", async () => {
    const fundId = new Uint8Array(32);
    fundId[0] = 0xfd;
    fundId[1] = 0x03;
    const [fundPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fund"), fakeTrust.toBuffer(), Buffer.from(fundId)],
      program.programId,
    );

    const manager = Keypair.generate();
    const sig = await provider.connection.requestAirdrop(manager.publicKey, 1_000_000_000);
    await provider.connection.confirmTransaction(sig);

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
      .createFund(Array.from(fundId), 2000)
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        fund: fundPda,
        quoteMint,
        manager: manager.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([manager])
      .rpc();

    // Down-mark from 0 to 0 (HWM stays 0) — carry must not accrue.
    await program.methods
      .updateNav(new anchor.BN(0))
      .accounts({ fund: fundPda, manager: manager.publicKey })
      .signers([manager])
      .rpc();

    const f = await program.account.fund.fetch(fundPda);
    expect(f.accruedCarry.toString()).to.eq("0");
    expect(f.highWaterMark.toString()).to.eq("0");
  });
});
