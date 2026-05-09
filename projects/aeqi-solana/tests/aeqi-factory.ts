import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiFactory } from "../target/types/aeqi_factory";
import { AeqiTrust } from "../target/types/aeqi_trust";
import { AeqiRole } from "../target/types/aeqi_role";
import { AeqiToken } from "../target/types/aeqi_token";
import { AeqiGovernance } from "../target/types/aeqi_governance";
import { AeqiTreasury } from "../target/types/aeqi_treasury";
import { AeqiVesting } from "../target/types/aeqi_vesting";
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
  const treasury = anchor.workspace.aeqiTreasury as Program<AeqiTreasury>;
  const vesting = anchor.workspace.aeqiVesting as Program<AeqiVesting>;

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

  it("max_supply_cap from TokenInitConfig is enforced by mint_tokens", async () => {
    // createCompanyFull → create_mint → mint up to cap → exceed (fails) →
    // residual headroom mint succeeds. Cap = 2000, decimals = 0 to keep
    // the math literal.
    const trustId = new Uint8Array(32);
    trustId[0] = 0xca;
    trustId[1] = 0xaa;

    const [trustPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustId)],
      trust.programId,
    );
    const roleModuleId = new Uint8Array(32); roleModuleId[0] = 0x52;
    const tokenModuleId = new Uint8Array(32); tokenModuleId[0] = 0x54;
    const govModuleId = new Uint8Array(32); govModuleId[0] = 0x47;

    const pdaModule = (id: Uint8Array) =>
      PublicKey.findProgramAddressSync(
        [Buffer.from("module"), trustPda.toBuffer(), Buffer.from(id)],
        trust.programId,
      )[0];
    const roleModulePda = pdaModule(roleModuleId);
    const tokenModulePda = pdaModule(tokenModuleId);
    const govModulePda = pdaModule(govModuleId);

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
        0, // decimals
        new anchor.BN(2000), // max_supply_cap
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

    // Now create the Token-2022 mint + an ATA, then try mints.
    const {
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
      getAssociatedTokenAddressSync,
      createAssociatedTokenAccountInstruction,
      getAccount,
    } = await import("@solana/spl-token");

    const [mintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("token_authority"), trustPda.toBuffer()],
      token.programId,
    );
    const [mintPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("mint"), trustPda.toBuffer()],
      token.programId,
    );

    await token.methods
      .createMint(0)
      .accounts({
        trust: trustPda,
        moduleState: tokenModuleStatePda,
        mintAuthority: mintAuthorityPda,
        mint: mintPda,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const recipient = provider.wallet.publicKey;
    const ata = getAssociatedTokenAddressSync(
      mintPda,
      recipient,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    await provider.sendAndConfirm(
      new anchor.web3.Transaction().add(
        createAssociatedTokenAccountInstruction(
          provider.wallet.publicKey,
          ata,
          recipient,
          mintPda,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID,
        ),
      ),
    );

    // Mint 1500 (under cap) — succeeds.
    await token.methods
      .mintTokens(new anchor.BN(1500))
      .accounts({
        trust: trustPda,
        moduleState: tokenModuleStatePda,
        mintAuthority: mintAuthorityPda,
        mint: mintPda,
        recipientTa: ata,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();
    let acct = await getAccount(provider.connection, ata, undefined, TOKEN_2022_PROGRAM_ID);
    expect(acct.amount.toString()).to.eq("1500");

    // Mint 600 more (1500+600=2100 > 2000) — must fail.
    let threw = false;
    try {
      await token.methods
        .mintTokens(new anchor.BN(600))
        .accounts({
          trust: trustPda,
          moduleState: tokenModuleStatePda,
          mintAuthority: mintAuthorityPda,
          mint: mintPda,
          recipientTa: ata,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .rpc();
    } catch (e: any) {
      threw = true;
      expect(e.toString()).to.match(/SupplyCapExceeded/);
    }
    expect(threw).to.eq(true);

    // Residual headroom: mint 500 (1500+500=2000 ≤ 2000) — succeeds.
    await token.methods
      .mintTokens(new anchor.BN(500))
      .accounts({
        trust: trustPda,
        moduleState: tokenModuleStatePda,
        mintAuthority: mintAuthorityPda,
        mint: mintPda,
        recipientTa: ata,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();
    acct = await getAccount(provider.connection, ata, undefined, TOKEN_2022_PROGRAM_ID);
    expect(acct.amount.toString()).to.eq("2000");
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

  // Canonical templates registry: prove that the on-chain factory supports
  // multiple distinct named templates registered side-by-side, each with a
  // different module set. BASIC = role + token + governance (the AEIQ
  // shape). VENTURE = BASIC + treasury + vesting (the cap-table-company
  // shape that holds funds + has time-vested grants).
  //
  // What `instantiate_template` ships today: trust.initialize +
  // register_module per template-spec'd module + trust.finalize. Per-module
  // init/finalize/set_bytes_config remains a separate caller step (each
  // module's own context shape varies). This test asserts the registry +
  // instantiation work; module init for the spawned trust is the next step
  // a real caller (or the platform bridge) would run.
  it("registers BASIC + VENTURE templates side-by-side and instantiates both", async () => {
    const BASIC_ID = (() => { const k = new Uint8Array(32); k[0]=0x42; k[1]=0x53; k[2]=0x43; return k; })(); // 'BSC'
    const VENTURE_ID = (() => { const k = new Uint8Array(32); k[0]=0x56; k[1]=0x4e; k[2]=0x54; return k; })(); // 'VNT'

    const moduleIdR = new Uint8Array(32); moduleIdR[0] = 0x52;  // 'R' role
    const moduleIdT = new Uint8Array(32); moduleIdT[0] = 0x54;  // 'T' token
    const moduleIdG = new Uint8Array(32); moduleIdG[0] = 0x47;  // 'G' governance
    const moduleIdY = new Uint8Array(32); moduleIdY[0] = 0x59;  // 'Y' treasury (no 'T' clash)
    const moduleIdV = new Uint8Array(32); moduleIdV[0] = 0x56;  // 'V' vesting

    // BASIC: role + token + governance
    const [basicPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("template"), Buffer.from(BASIC_ID)],
      factory.programId,
    );
    await factory.methods
      .registerTemplate(
        Array.from(BASIC_ID),
        [
          { moduleId: Array.from(moduleIdR), programId: role.programId,       trustAcl: new anchor.BN(0xff) },
          { moduleId: Array.from(moduleIdT), programId: token.programId,      trustAcl: new anchor.BN(0xff) },
          { moduleId: Array.from(moduleIdG), programId: governance.programId, trustAcl: new anchor.BN(0xff) },
        ],
        [],
      )
      .accounts({
        template: basicPda,
        admin: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // VENTURE: role + token + governance + treasury + vesting
    const [venturePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("template"), Buffer.from(VENTURE_ID)],
      factory.programId,
    );
    await factory.methods
      .registerTemplate(
        Array.from(VENTURE_ID),
        [
          { moduleId: Array.from(moduleIdR), programId: role.programId,       trustAcl: new anchor.BN(0xff) },
          { moduleId: Array.from(moduleIdT), programId: token.programId,      trustAcl: new anchor.BN(0xff) },
          { moduleId: Array.from(moduleIdG), programId: governance.programId, trustAcl: new anchor.BN(0xff) },
          { moduleId: Array.from(moduleIdY), programId: treasury.programId,   trustAcl: new anchor.BN(0xff) },
          { moduleId: Array.from(moduleIdV), programId: vesting.programId,    trustAcl: new anchor.BN(0xff) },
        ],
        [],
      )
      .accounts({
        template: venturePda,
        admin: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const basic = await factory.account.template.fetch(basicPda);
    expect(basic.modules.length).to.eq(3);
    const venture = await factory.account.template.fetch(venturePda);
    expect(venture.modules.length).to.eq(5);

    // Instantiate BASIC against a fresh trust.
    const trustIdBasic = new Uint8Array(32); trustIdBasic[0] = 0xb1; trustIdBasic[1] = 0x42;
    const [trustPdaBasic] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustIdBasic)],
      trust.programId,
    );
    const basicModulePdas = [moduleIdR, moduleIdT, moduleIdG].map((id) =>
      PublicKey.findProgramAddressSync(
        [Buffer.from("module"), trustPdaBasic.toBuffer(), Buffer.from(id)],
        trust.programId,
      )[0],
    );
    await factory.methods
      .instantiateTemplate(Array.from(trustIdBasic))
      .accounts({
        template: basicPda,
        trust: trustPdaBasic,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts(
        basicModulePdas.map((p) => ({ pubkey: p, isWritable: true, isSigner: false })),
      )
      .rpc();

    const tBasic = await trust.account.trust.fetch(trustPdaBasic);
    expect(tBasic.creationMode).to.eq(false);
    expect(tBasic.moduleCount).to.eq(3);
    // Sanity: each module record points at the right program.
    expect((await trust.account.module.fetch(basicModulePdas[0])).programId.toBase58())
      .to.eq(role.programId.toBase58());
    expect((await trust.account.module.fetch(basicModulePdas[1])).programId.toBase58())
      .to.eq(token.programId.toBase58());
    expect((await trust.account.module.fetch(basicModulePdas[2])).programId.toBase58())
      .to.eq(governance.programId.toBase58());

    // Instantiate VENTURE against a different fresh trust — proves the same
    // factory can spawn distinct shapes from distinct registered templates.
    const trustIdVent = new Uint8Array(32); trustIdVent[0] = 0xb2; trustIdVent[1] = 0x56;
    const [trustPdaVent] = PublicKey.findProgramAddressSync(
      [Buffer.from("trust"), Buffer.from(trustIdVent)],
      trust.programId,
    );
    const ventureModulePdas = [moduleIdR, moduleIdT, moduleIdG, moduleIdY, moduleIdV].map((id) =>
      PublicKey.findProgramAddressSync(
        [Buffer.from("module"), trustPdaVent.toBuffer(), Buffer.from(id)],
        trust.programId,
      )[0],
    );
    await factory.methods
      .instantiateTemplate(Array.from(trustIdVent))
      .accounts({
        template: venturePda,
        trust: trustPdaVent,
        authority: provider.wallet.publicKey,
        aeqiTrustProgram: trust.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts(
        ventureModulePdas.map((p) => ({ pubkey: p, isWritable: true, isSigner: false })),
      )
      .rpc();

    const tVent = await trust.account.trust.fetch(trustPdaVent);
    expect(tVent.creationMode).to.eq(false);
    expect(tVent.moduleCount).to.eq(5);
    expect((await trust.account.module.fetch(ventureModulePdas[3])).programId.toBase58())
      .to.eq(treasury.programId.toBase58());
    expect((await trust.account.module.fetch(ventureModulePdas[4])).programId.toBase58())
      .to.eq(vesting.programId.toBase58());
  });
});
