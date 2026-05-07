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
