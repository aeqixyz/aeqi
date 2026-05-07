import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiFactory } from "../target/types/aeqi_factory";
import { AeqiTrust } from "../target/types/aeqi_trust";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_factory", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const factory = anchor.workspace.aeqiFactory as Program<AeqiFactory>;
  const trust = anchor.workspace.aeqiTrust as Program<AeqiTrust>;

  it("create_company spawns a trust via CPI to aeqi_trust::initialize", async () => {
    const trustId = new Uint8Array(32);
    trustId[0] = 0x42; // distinguish from the trust suite's trust

    const [trustPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      trust.programId,
    );

    await factory.methods
      .createCompany(Array.from(trustId))
      .accounts({
        trust: trustPda,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Verify aeqi_trust state was actually written by the CPI.
    const trustAcct = await trust.account.trust.fetch(trustPda);
    expect(trustAcct.creationMode).to.eq(true);
    expect(trustAcct.authority.toBase58()).to.eq(
      provider.wallet.publicKey.toBase58(),
    );
    expect(trustAcct.moduleCount).to.eq(0);
    expect(Buffer.from(trustAcct.trustId).toString("hex")).to.eq(
      Buffer.from(trustId).toString("hex"),
    );
  });

  it("register_template stores a Template PDA", async () => {
    const templateId = new Uint8Array(32);
    templateId[0] = 0xaa;
    templateId[1] = 0xbb;

    const [templatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("template"), Buffer.from(templateId)],
      factory.programId,
    );

    const moduleId1 = new Uint8Array(32);
    moduleId1[0] = 0x52; // 'R'

    await factory.methods
      .registerTemplate(
        Array.from(templateId),
        [
          {
            moduleId: Array.from(moduleId1),
            programId: anchor.web3.Keypair.generate().publicKey,
            trustAcl: new anchor.BN(0xff),
          },
        ],
        [],
      )
      .accounts({
        template: templatePda,
        admin: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const tmpl = await factory.account.template.fetch(templatePda);
    expect(tmpl.modules.length).to.eq(1);
    expect(tmpl.aclEdges.length).to.eq(0);
    expect(tmpl.admin.toBase58()).to.eq(provider.wallet.publicKey.toBase58());
  });

  it("create_with_modules atomically spawns trust + N modules + finalizes", async () => {
    const trustId = new Uint8Array(32);
    trustId[0] = 0x77;

    const [trustPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      trust.programId,
    );

    const moduleIdRole = new Uint8Array(32);
    moduleIdRole[0] = 0x52; // 'R'
    const moduleIdGov = new Uint8Array(32);
    moduleIdGov[0] = 0x47; // 'G'

    const [modulePdaRole] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(moduleIdRole)],
      trust.programId,
    );
    const [modulePdaGov] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(moduleIdGov)],
      trust.programId,
    );

    const dummyRoleProg = anchor.web3.Keypair.generate().publicKey;
    const dummyGovProg = anchor.web3.Keypair.generate().publicKey;

    await factory.methods
      .createWithModules(Array.from(trustId), [
        {
          moduleId: Array.from(moduleIdRole),
          programId: dummyRoleProg,
          trustAcl: new anchor.BN(0xff),
        },
        {
          moduleId: Array.from(moduleIdGov),
          programId: dummyGovProg,
          trustAcl: new anchor.BN(0x80),
        },
      ])
      .accounts({
        trust: trustPda,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts([
        { pubkey: modulePdaRole, isWritable: true, isSigner: false },
        { pubkey: modulePdaGov, isWritable: true, isSigner: false },
      ])
      .rpc();

    // Trust state — finalized, 2 modules registered
    const trustAcct = await trust.account.trust.fetch(trustPda);
    expect(trustAcct.creationMode).to.eq(false); // finalized
    expect(trustAcct.moduleCount).to.eq(2);

    // Both module PDAs were created with the right program IDs and ACLs
    const role = await trust.account.module.fetch(modulePdaRole);
    expect(role.programId.toBase58()).to.eq(dummyRoleProg.toBase58());
    expect(role.trustAcl.toString()).to.eq("255");

    const gov = await trust.account.module.fetch(modulePdaGov);
    expect(gov.programId.toBase58()).to.eq(dummyGovProg.toBase58());
    expect(gov.trustAcl.toString()).to.eq("128");
  });

  it("rejects register_template with empty module set", async () => {
    const templateId = new Uint8Array(32);
    templateId[0] = 0xee;

    const [templatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("template"), Buffer.from(templateId)],
      factory.programId,
    );

    let threw = false;
    try {
      await factory.methods
        .registerTemplate(Array.from(templateId), [], [])
        .accounts({
          template: templatePda,
          admin: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/EmptyModuleSet/);
    }
    expect(threw).to.eq(true);
  });
});
