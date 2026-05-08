import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiFactory } from "../target/types/aeqi_factory";
import { AeqiTrust } from "../target/types/aeqi_trust";
import { AeqiRole } from "../target/types/aeqi_role";
import { AeqiToken } from "../target/types/aeqi_token";
import { AeqiGovernance } from "../target/types/aeqi_governance";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_factory", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const factory = anchor.workspace.aeqiFactory as Program<AeqiFactory>;
  const trust = anchor.workspace.aeqiTrust as Program<AeqiTrust>;
  const role = anchor.workspace.aeqiRole as Program<AeqiRole>;
  const token = anchor.workspace.aeqiToken as Program<AeqiToken>;
  const governance = anchor.workspace.aeqiGovernance as Program<AeqiGovernance>;

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

  it("instantiate_template replays a registered template into a fresh TRUST", async () => {
    // Register a template first
    const templateId = new Uint8Array(32);
    templateId[0] = 0xa1;
    templateId[1] = 0x02;

    const [templatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("template"), Buffer.from(templateId)],
      factory.programId,
    );

    const moduleIdR = new Uint8Array(32);
    moduleIdR[0] = 0x52;
    moduleIdR[1] = 0xa1;
    const moduleIdT = new Uint8Array(32);
    moduleIdT[0] = 0x54;
    moduleIdT[1] = 0xa1;

    const programR = anchor.web3.Keypair.generate().publicKey;
    const programT = anchor.web3.Keypair.generate().publicKey;

    await factory.methods
      .registerTemplate(
        Array.from(templateId),
        [
          {
            moduleId: Array.from(moduleIdR),
            programId: programR,
            trustAcl: new anchor.BN(0xff),
          },
          {
            moduleId: Array.from(moduleIdT),
            programId: programT,
            trustAcl: new anchor.BN(0x80),
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

    // Now instantiate it against a fresh trust_id
    const trustId = new Uint8Array(32);
    trustId[0] = 0x88;
    trustId[1] = 0xa1;

    const [trustPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      trust.programId,
    );

    const [modR] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(moduleIdR)],
      trust.programId,
    );
    const [modT] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(moduleIdT)],
      trust.programId,
    );

    await factory.methods
      .instantiateTemplate(Array.from(trustId))
      .accounts({
        template: templatePda,
        trust: trustPda,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts([
        { pubkey: modR, isWritable: true, isSigner: false },
        { pubkey: modT, isWritable: true, isSigner: false },
      ])
      .rpc();

    // Verify trust is finalized + 2 modules registered with right program IDs
    const t = await trust.account.trust.fetch(trustPda);
    expect(t.creationMode).to.eq(false);
    expect(t.moduleCount).to.eq(2);

    const mR = await trust.account.module.fetch(modR);
    expect(mR.programId.toBase58()).to.eq(programR.toBase58());
    expect(mR.trustAcl.toString()).to.eq("255");

    const mT = await trust.account.module.fetch(modT);
    expect(mT.programId.toBase58()).to.eq(programT.toBase58());
    expect(mT.trustAcl.toString()).to.eq("128");
  });

  it("create_company_full atomically spawns trust + registers + inits 3 modules in ONE tx", async () => {
    // This is the EVM Factory._createTRUST shape — atomic orchestration.
    // Currently 4 of the 5 EVM steps run in one tx:
    //   1. trust.initialize
    //   2. trust.register_module ×3 (role / token / governance)
    //   3. each module's init (creates module-state PDA)
    //   4. trust.finalize
    // Step 5 (each module's finalize with config bytes) lands when the
    // BytesConfig dispatch flow ships.

    const trustId = new Uint8Array(32);
    trustId[0] = 0x99;
    trustId[1] = 0xaa;

    const roleModuleId = new Uint8Array(32);
    roleModuleId[0] = 0x52;
    const tokenModuleId = new Uint8Array(32);
    tokenModuleId[0] = 0x54;
    const govModuleId = new Uint8Array(32);
    govModuleId[0] = 0x47;

    const [trustPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      trust.programId,
    );
    const [roleModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(roleModuleId)],
      trust.programId,
    );
    const [tokenModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(tokenModuleId)],
      trust.programId,
    );
    const [govModulePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(govModuleId)],
      trust.programId,
    );

    const [roleModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_module"), trustPda.toBuffer()],
      role.programId,
    );
    const [tokenModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_module"), trustPda.toBuffer()],
      token.programId,
    );
    const [govModuleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("gov_module"), trustPda.toBuffer()],
      governance.programId,
    );

    // BytesConfig PDA for the token module's borsh-encoded TokenInitConfig.
    // Lives under aeqi_trust's program id; key is TOKEN_CONFIG_KEY = [1,0,...,0].
    const tokenConfigKey = new Uint8Array(32);
    tokenConfigKey[0] = 1;
    const [tokenBytesConfigPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("cfg_bytes"), trustPda.toBuffer(), Buffer.from(tokenConfigKey)],
      trust.programId,
    );

    await factory.methods
      .createCompanyFull(
        Array.from(trustId),
        Array.from(roleModuleId),
        Array.from(tokenModuleId),
        Array.from(govModuleId),
        new anchor.BN(0xff),
        new anchor.BN(0xff),
        new anchor.BN(0xff),
        9, // token_decimals
        new anchor.BN(1_000_000_000), // token_max_supply_cap
      )
      .accounts({
        trust: trustPda,
        roleModule: roleModulePda,
        tokenModule: tokenModulePda,
        govModule: govModulePda,
        roleModuleState: roleModuleStatePda,
        tokenModuleState: tokenModuleStatePda,
        govModuleState: govModuleStatePda,
        tokenBytesConfig: tokenBytesConfigPda,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        aeqiRoleProgram: role.programId,
        aeqiTokenProgram: token.programId,
        aeqiGovernanceProgram: governance.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Verify trust is finalized + 3 modules registered
    const t = await trust.account.trust.fetch(trustPda);
    expect(t.creationMode).to.eq(false);
    expect(t.moduleCount).to.eq(3);

    // Each module record has the right program ID
    const r = await trust.account.module.fetch(roleModulePda);
    expect(r.programId.toBase58()).to.eq(role.programId.toBase58());
    const tk = await trust.account.module.fetch(tokenModulePda);
    expect(tk.programId.toBase58()).to.eq(token.programId.toBase58());
    const g = await trust.account.module.fetch(govModulePda);
    expect(g.programId.toBase58()).to.eq(governance.programId.toBase58());

    // Each module's state PDA was created — module init ran
    const rs = await role.account.roleModuleState.fetch(roleModuleStatePda);
    expect(rs.trust.toBase58()).to.eq(trustPda.toBase58());
    expect(rs.initialized).to.eq(true);

    const ts = await token.account.tokenModuleState.fetch(tokenModuleStatePda);
    expect(ts.trust.toBase58()).to.eq(trustPda.toBase58());
    // BytesConfig dispatch landed: finalize decoded the blob and copied
    // decimals + max_supply_cap onto module_state.
    expect(ts.decimals).to.eq(9);
    expect(ts.maxSupplyCap.toString()).to.eq("1000000000");

    const gs = await governance.account.governanceModuleState.fetch(govModuleStatePda);
    expect(gs.trust.toBase58()).to.eq(trustPda.toBase58());
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
