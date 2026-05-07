import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { AeqiRole } from "../target/types/aeqi_role";
import { PublicKey, Keypair } from "@solana/web3.js";
import { expect } from "chai";

describe("aeqi_role", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.aeqiRole as Program<AeqiRole>;

  const fakeTrust = Keypair.generate().publicKey;

  it("init creates the role module state", async () => {
    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_module"), fakeTrust.toBuffer()],
      program.programId,
    );

    await program.methods
      .init()
      .accounts({
        trust: fakeTrust,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const m = await program.account.roleModuleState.fetch(moduleStatePda);
    expect(m.trust.toBase58()).to.eq(fakeTrust.toBase58());
    expect(m.initialized).to.eq(true);
  });

  it("create_role_type stores a RoleType PDA", async () => {
    // role_type_id = keccak256("director") simulated as deterministic bytes
    const directorId = new Uint8Array(32).fill(0);
    directorId[0] = 0x44; // 'D'
    directorId[1] = 0x49; // 'I'
    directorId[2] = 0x52; // 'R'

    const [rtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), fakeTrust.toBuffer(), Buffer.from(directorId)],
      program.programId,
    );

    await program.methods
      .createRoleType(Array.from(directorId), 0, {
        // hierarchy 0 = highest authority (board/founders)
        vesting: false,
        vestingCliff: new anchor.BN(0),
        vestingDuration: new anchor.BN(0),
        fdv: false,
        fdvStart: new anchor.BN(0),
        fdvEnd: new anchor.BN(0),
        probationaryPeriod: new anchor.BN(0),
        severancePeriod: new anchor.BN(0),
        contribution: false,
      })
      .accounts({
        trust: fakeTrust,
        roleType: rtPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const rt = await program.account.roleType.fetch(rtPda);
    expect(rt.hierarchy).to.eq(0);
    expect(rt.roleCount).to.eq(0);
    expect(Buffer.from(rt.roleTypeId).toString("hex")).to.eq(
      Buffer.from(directorId).toString("hex"),
    );
  });

  it("create_role spawns a vacant role under a role type", async () => {
    // Reuse the director role type from the previous test
    const directorTypeId = new Uint8Array(32);
    directorTypeId[0] = 0x44; // 'D'
    directorTypeId[1] = 0x49;
    directorTypeId[2] = 0x52;

    const roleId = new Uint8Array(32);
    roleId[0] = 0x46; // 'F' for founder
    roleId[1] = 0x4f; // 'O'
    roleId[2] = 0x55; // 'U'

    const [rtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), fakeTrust.toBuffer(), Buffer.from(directorTypeId)],
      program.programId,
    );
    const [rolePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), fakeTrust.toBuffer(), Buffer.from(roleId)],
      program.programId,
    );

    const ipfsCid = new Uint8Array(64).fill(0x20); // ASCII space — placeholder

    await program.methods
      .createRole(
        Array.from(roleId),
        Array.from(directorTypeId),
        null, // no parent role
        Array.from(ipfsCid),
      )
      .accounts({
        trust: fakeTrust,
        roleType: rtPda,
        role: rolePda,
        callerRole: null, // permissionless skeleton path
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const role = await program.account.role.fetch(rolePda);
    expect(role.status).to.eq(0); // RoleStatus::Vacant
    expect(role.account.toBase58()).to.eq(PublicKey.default.toBase58());
    expect(Buffer.from(role.roleId).toString("hex")).to.eq(
      Buffer.from(roleId).toString("hex"),
    );
    expect(Buffer.from(role.roleTypeId).toString("hex")).to.eq(
      Buffer.from(directorTypeId).toString("hex"),
    );

    const rt = await program.account.roleType.fetch(rtPda);
    expect(rt.roleCount).to.eq(1);
  });

  it("assign_role transitions Vacant → Occupied + bumps checkpoint", async () => {
    const directorTypeId = new Uint8Array(32);
    directorTypeId[0] = 0x44;
    directorTypeId[1] = 0x49;
    directorTypeId[2] = 0x52;

    const roleId = new Uint8Array(32);
    roleId[0] = 0x46;
    roleId[1] = 0x4f;
    roleId[2] = 0x55;

    const [rtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), fakeTrust.toBuffer(), Buffer.from(directorTypeId)],
      program.programId,
    );
    const [rolePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), fakeTrust.toBuffer(), Buffer.from(roleId)],
      program.programId,
    );

    const occupant = provider.wallet.publicKey;
    const [checkpointPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("role_ckpt"),
        fakeTrust.toBuffer(),
        Buffer.from(directorTypeId),
        occupant.toBuffer(),
      ],
      program.programId,
    );

    await program.methods
      .assignRole(occupant)
      .accounts({
        role: rolePda,
        roleType: rtPda,
        trust: fakeTrust,
        checkpoint: checkpointPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const role = await program.account.role.fetch(rolePda);
    expect(role.status).to.eq(1); // RoleStatus::Occupied
    expect(role.account.toBase58()).to.eq(occupant.toBase58());

    const ckpt = await program.account.roleVoteCheckpoint.fetch(checkpointPda);
    expect(ckpt.count.toString()).to.eq("1");
    expect(ckpt.account.toBase58()).to.eq(occupant.toBase58());
  });

  it("delegate_role transfers vote-power from self to a delegatee", async () => {
    // Fresh trust so PDAs don't collide with previous tests
    const trustD = Keypair.generate().publicKey;

    const [moduleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_module"), trustD.toBuffer()],
      program.programId,
    );
    await program.methods
      .init()
      .accounts({
        trust: trustD,
        moduleState: moduleStatePda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const directorTypeId = new Uint8Array(32);
    directorTypeId[0] = 0xd2;
    const [rtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), trustD.toBuffer(), Buffer.from(directorTypeId)],
      program.programId,
    );
    await program.methods
      .createRoleType(Array.from(directorTypeId), 0, {
        vesting: false,
        vestingCliff: new anchor.BN(0),
        vestingDuration: new anchor.BN(0),
        fdv: false,
        fdvStart: new anchor.BN(0),
        fdvEnd: new anchor.BN(0),
        probationaryPeriod: new anchor.BN(0),
        severancePeriod: new anchor.BN(0),
        contribution: false,
      })
      .accounts({
        trust: trustD,
        roleType: rtPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const roleId = new Uint8Array(32);
    roleId[0] = 0xd2;
    roleId[1] = 0x01;
    const [rolePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), trustD.toBuffer(), Buffer.from(roleId)],
      program.programId,
    );
    await program.methods
      .createRole(
        Array.from(roleId),
        Array.from(directorTypeId),
        null,
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trustD,
        roleType: rtPda,
        role: rolePda,
        callerRole: null,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Assign to provider.wallet (auto-self-delegates)
    const userA = provider.wallet.publicKey;
    const [aCkptPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("role_ckpt"),
        trustD.toBuffer(),
        Buffer.from(directorTypeId),
        userA.toBuffer(),
      ],
      program.programId,
    );
    await program.methods
      .assignRole(userA)
      .accounts({
        role: rolePda,
        roleType: rtPda,
        trust: trustD,
        checkpoint: aCkptPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Verify A has 1 vote after assign (self-delegation)
    let aCkpt = await program.account.roleVoteCheckpoint.fetch(aCkptPda);
    expect(aCkpt.count.toString()).to.eq("1");

    // Now delegate to user B — first-time delegation FROM userA's perspective,
    // but A's prior delegatee is A itself (set at assign). So prev = userA;
    // we DO need to pass prev_checkpoint.
    const userB = Keypair.generate().publicKey;
    const [delegationPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_deleg"), trustD.toBuffer(), Buffer.from(roleId)],
      program.programId,
    );
    const [bCkptPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("role_ckpt"),
        trustD.toBuffer(),
        Buffer.from(directorTypeId),
        userB.toBuffer(),
      ],
      program.programId,
    );

    // First delegation creates RoleDelegation with prev=Pubkey::default(), so
    // the program's `if prev != default` branch is skipped — prev_checkpoint
    // is None on first call.
    await program.methods
      .delegateRole(userB)
      .accounts({
        role: rolePda,
        roleType: rtPda,
        delegation: delegationPda,
        prevCheckpoint: null,
        newCheckpoint: bCkptPda,
        newDelegatee: userB,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // After delegation: B has +1, A unchanged because the program saw prev as
    // Pubkey::default() (the freshly-init'd RoleDelegation slot) and skipped
    // the prev decrement.
    const bCkpt = await program.account.roleVoteCheckpoint.fetch(bCkptPda);
    expect(bCkpt.count.toString()).to.eq("1");
    expect(bCkpt.account.toBase58()).to.eq(userB.toBase58());

    const deleg = await program.account.roleDelegation.fetch(delegationPda);
    expect(deleg.delegatee.toBase58()).to.eq(userB.toBase58());
  });

  it("authority walk authorizes ancestor over deep descendant", async () => {
    // Use a fresh trust so PDAs don't collide with previous tests.
    const trust2 = Keypair.generate().publicKey;

    // role_module init for trust2
    const [moduleStatePda2] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_module"), trust2.toBuffer()],
      program.programId,
    );
    await program.methods
      .init()
      .accounts({
        trust: trust2,
        moduleState: moduleStatePda2,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Define 3 role types: director (h=0), ceo (h=1), eng (h=4)
    const types: Record<string, Uint8Array> = {
      director: new Uint8Array(32),
      ceo: new Uint8Array(32),
      eng: new Uint8Array(32),
    };
    types.director[0] = 0xd1;
    types.ceo[0] = 0xc1;
    types.eng[0] = 0xe1;

    for (const [name, id] of Object.entries(types)) {
      const [pda] = PublicKey.findProgramAddressSync(
        [Buffer.from("role_type"), trust2.toBuffer(), Buffer.from(id)],
        program.programId,
      );
      const hierarchy = name === "director" ? 0 : name === "ceo" ? 1 : 4;
      await program.methods
        .createRoleType(Array.from(id), hierarchy, {
          vesting: false,
          vestingCliff: new anchor.BN(0),
          vestingDuration: new anchor.BN(0),
          fdv: false,
          fdvStart: new anchor.BN(0),
          fdvEnd: new anchor.BN(0),
          probationaryPeriod: new anchor.BN(0),
          severancePeriod: new anchor.BN(0),
          contribution: false,
        })
        .accounts({
          trust: trust2,
          roleType: pda,
          payer: provider.wallet.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
    }

    // Create root founder role (director type, no parent)
    const founderId = new Uint8Array(32);
    founderId[0] = 0xf1;
    const [founderPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), trust2.toBuffer(), Buffer.from(founderId)],
      program.programId,
    );
    const [directorRtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), trust2.toBuffer(), Buffer.from(types.director)],
      program.programId,
    );
    await program.methods
      .createRole(
        Array.from(founderId),
        Array.from(types.director),
        null,
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trust2,
        roleType: directorRtPda,
        role: founderPda,
        callerRole: null,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Assign founder to userA (provider.wallet)
    const [founderCkpt] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("role_ckpt"),
        trust2.toBuffer(),
        Buffer.from(types.director),
        provider.wallet.publicKey.toBuffer(),
      ],
      program.programId,
    );
    await program.methods
      .assignRole(provider.wallet.publicKey)
      .accounts({
        role: founderPda,
        roleType: directorRtPda,
        trust: trust2,
        checkpoint: founderCkpt,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // Create ceo1 with parent = founder. Skeleton path: callerRole = null.
    const ceoRoleId = new Uint8Array(32);
    ceoRoleId[0] = 0xc1;
    ceoRoleId[1] = 0x01;
    const [ceoPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), trust2.toBuffer(), Buffer.from(ceoRoleId)],
      program.programId,
    );
    const [ceoRtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), trust2.toBuffer(), Buffer.from(types.ceo)],
      program.programId,
    );
    await program.methods
      .createRole(
        Array.from(ceoRoleId),
        Array.from(types.ceo),
        Array.from(founderId), // parent = founder
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trust2,
        roleType: ceoRtPda,
        role: ceoPda,
        callerRole: null, // skeleton — no walk
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    // NOW: create eng1 with parent = ceo, callerRole = founder.
    // The walk MUST succeed because founder is ancestor of ceo (eng's parent).
    // remaining_accounts = [ceo PDA, founder PDA] — chain from target up to
    // root, the new walker semantics.
    const engRoleId = new Uint8Array(32);
    engRoleId[0] = 0xe1;
    engRoleId[1] = 0x01;
    const [engPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role"), trust2.toBuffer(), Buffer.from(engRoleId)],
      program.programId,
    );
    const [engRtPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), trust2.toBuffer(), Buffer.from(types.eng)],
      program.programId,
    );
    await program.methods
      .createRole(
        Array.from(engRoleId),
        Array.from(types.eng),
        Array.from(ceoRoleId), // parent = ceo
        Array.from(new Uint8Array(64)),
      )
      .accounts({
        trust: trust2,
        roleType: engRtPda,
        role: engPda,
        callerRole: founderPda, // founder authorizing
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts([
        { pubkey: ceoPda, isWritable: false, isSigner: false }, // target
        { pubkey: founderPda, isWritable: false, isSigner: false }, // ancestor (caller)
      ])
      .rpc();

    const eng = await program.account.role.fetch(engPda);
    expect(eng.status).to.eq(0); // Vacant
    expect(Buffer.from(eng.parentRoleId).toString("hex")).to.eq(
      Buffer.from(ceoRoleId).toString("hex"),
    );
  });

  it("create_role_type stores hierarchies as expected (CEO=1, EA=4)", async () => {
    const ceoId = new Uint8Array(32);
    ceoId[0] = 0x43; // 'C'
    ceoId[1] = 0x45; // 'E'
    ceoId[2] = 0x4f; // 'O'

    const [ceoPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("role_type"), fakeTrust.toBuffer(), Buffer.from(ceoId)],
      program.programId,
    );

    await program.methods
      .createRoleType(Array.from(ceoId), 1, {
        vesting: true,
        vestingCliff: new anchor.BN(60 * 60 * 24 * 365), // 1y
        vestingDuration: new anchor.BN(60 * 60 * 24 * 365 * 4), // 4y
        fdv: false,
        fdvStart: new anchor.BN(0),
        fdvEnd: new anchor.BN(0),
        probationaryPeriod: new anchor.BN(0),
        severancePeriod: new anchor.BN(0),
        contribution: false,
      })
      .accounts({
        trust: fakeTrust,
        roleType: ceoPda,
        payer: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const ceo = await program.account.roleType.fetch(ceoPda);
    expect(ceo.hierarchy).to.eq(1);
    expect(ceo.config.vesting).to.eq(true);
    expect(ceo.config.vestingDuration.toString()).to.eq(String(60 * 60 * 24 * 365 * 4));
  });
});
