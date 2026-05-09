import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiBudget } from "../target/types/aeqi_budget";
import { PublicKey, Keypair } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_budget", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiBudget as Program<AeqiBudget>;

  const fakeTrust = Keypair.generate().publicKey;
  let modulePda: PublicKey;

  before(() => {
    [modulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("budget_module"), fakeTrust.toBuffer()],
      program.programId,
    );
  });

  it("init creates the budget module state", async () => {
    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.budgetModuleState.fetch(modulePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.budgetCount.toString()).to.eq("0");
  });

  it("create_budget + record_spend tracks allocation against cap", async () => {
    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xb1;

    const targetRoleId = new Uint8Array(32);
    targetRoleId[0] = 0x65; // 'e' for eng

    const [budgetPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("budget"), fakeTrust.toBuffer(), Buffer.from(budgetId)],
      program.programId,
    );

    await program.methods
      .createBudget(
        Array.from(budgetId),
        Array.from(targetRoleId),
        new anchor.BN(50_000),
        new anchor.BN(0), // no expiry
        null, // no parent
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        budget: budgetPda,
        grantor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    let b = await program.account.budget.fetch(budgetPda);
    expect(b.amount.toString()).to.eq("50000");
    expect(b.spent.toString()).to.eq("0");
    expect(b.frozen).to.eq(false);

    // Record two spends: 10000 + 25000 = 35000
    await program.methods
      .recordSpend(new anchor.BN(10_000))
      .accounts({
        budget: budgetPda,
        spender: provider.wallet.publicKey,
      })
      .rpc();
    await program.methods
      .recordSpend(new anchor.BN(25_000))
      .accounts({
        budget: budgetPda,
        spender: provider.wallet.publicKey,
      })
      .rpc();

    b = await program.account.budget.fetch(budgetPda);
    expect(b.spent.toString()).to.eq("35000");

    // Try to overspend (35000 + 20000 > 50000 cap)
    let threw = false;
    try {
      await program.methods
        .recordSpend(new anchor.BN(20_000))
        .accounts({
          budget: budgetPda,
          spender: provider.wallet.publicKey,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/ExceedsAllocation/);
    }
    expect(threw).to.eq(true);
  });

  it("freeze blocks further spends", async () => {
    const budgetId = new Uint8Array(32);
    budgetId[0] = 0xb2;

    const [budgetPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("budget"), fakeTrust.toBuffer(), Buffer.from(budgetId)],
      program.programId,
    );

    await program.methods
      .createBudget(
        Array.from(budgetId),
        Array.from(new Uint8Array(32).fill(0x66)),
        new anchor.BN(1000),
        new anchor.BN(0),
        null,
      )
      .accounts({
        trust: fakeTrust,
        moduleState: modulePda,
        budget: budgetPda,
        grantor: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await program.methods
      .freeze()
      .accounts({
        budget: budgetPda,
        grantor: provider.wallet.publicKey,
      })
      .rpc();

    let threw = false;
    try {
      await program.methods
        .recordSpend(new anchor.BN(100))
        .accounts({
          budget: budgetPda,
          spender: provider.wallet.publicKey,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/BudgetFrozen/);
    }
    expect(threw).to.eq(true);

    // Unfreeze + spend works
    await program.methods
      .unfreeze()
      .accounts({
        budget: budgetPda,
        grantor: provider.wallet.publicKey,
      })
      .rpc();

    await program.methods
      .recordSpend(new anchor.BN(100))
      .accounts({
        budget: budgetPda,
        spender: provider.wallet.publicKey,
      })
      .rpc();

    const b = await program.account.budget.fetch(budgetPda);
    expect(b.spent.toString()).to.eq("100");
  });
});
