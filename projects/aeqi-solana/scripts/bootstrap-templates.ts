/**
 * Bootstrap canonical templates against a running Solana validator.
 *
 * Idempotent: each template's PDA is checked first; existing templates are
 * left alone (you can't re-register at the same PDA anyway). Run after
 * `anchor deploy --provider.cluster <url>` to seed the on-chain template
 * registry with the canonical shapes the platform UI expects.
 *
 * Usage:
 *   ANCHOR_PROVIDER_URL=http://127.0.0.1:8899 \
 *   ANCHOR_WALLET=~/.config/solana/id.json \
 *   npx ts-node scripts/bootstrap-templates.ts
 *
 * Adds:
 *   - BASIC   = role + token + governance         (AEQI-shape, 3 modules)
 *   - VENTURE = BASIC + treasury + vesting         (cap-table company, 5)
 *
 * Template ids are stable byte arrays: the same id produces the same PDA on
 * every chain, so the platform can resolve "BASIC" / "VENTURE" without
 * consulting any external registry.
 */
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { AeqiFactory } from "../target/types/aeqi_factory";
import { AeqiRole } from "../target/types/aeqi_role";
import { AeqiToken } from "../target/types/aeqi_token";
import { AeqiGovernance } from "../target/types/aeqi_governance";
import { AeqiTreasury } from "../target/types/aeqi_treasury";
import { AeqiVesting } from "../target/types/aeqi_vesting";

// Stable template ids. The first three bytes encode an ASCII handle for
// debug-readability; the remaining 29 bytes are zero. These are part of
// the on-chain protocol surface. Do not change them without a migration.
function templateIdFromHandle(handle: string): Uint8Array {
  if (handle.length > 32) throw new Error("handle must be <= 32 bytes");
  const id = new Uint8Array(32);
  for (let i = 0; i < handle.length; i++) id[i] = handle.charCodeAt(i);
  return id;
}

const BASIC_ID = templateIdFromHandle("BSC");
const VENTURE_ID = templateIdFromHandle("VNT");

// Module-id sub-keys inside a template. Same scheme: three-byte handle,
// padded. These names appear in the indexer + the bridge wizard, so keep
// them in sync if you change them.
const MODULE_ROLE = templateIdFromHandle("R");
const MODULE_TOKEN = templateIdFromHandle("T");
const MODULE_GOV = templateIdFromHandle("G");
const MODULE_TREASURY = templateIdFromHandle("Y"); // 'Y' to avoid clash with token 'T'
const MODULE_VESTING = templateIdFromHandle("V");

const FULL_ACL = new anchor.BN(0xff);

async function ensureTemplate(
  factory: Program<AeqiFactory>,
  templateId: Uint8Array,
  label: string,
  modules: { moduleId: Uint8Array; programId: PublicKey; trustAcl: anchor.BN }[],
  admin: PublicKey,
): Promise<{ pda: PublicKey; created: boolean }> {
  const [pda] = PublicKey.findProgramAddressSync(
    [Buffer.from("template"), Buffer.from(templateId)],
    factory.programId,
  );

  // Idempotency check: fetch the template; if it exists, skip.
  try {
    const existing = await factory.account.template.fetch(pda);
    if (existing.modules.length > 0) {
      console.log(
        `[skip]  ${label.padEnd(8)} already registered at ${pda.toBase58()} (${existing.modules.length} modules)`,
      );
      return { pda, created: false };
    }
  } catch (_) {
    // Account doesn't exist; fall through to register.
  }

  await factory.methods
    .registerTemplate(
      Array.from(templateId),
      modules.map((m) => ({
        moduleId: Array.from(m.moduleId),
        programId: m.programId,
        trustAcl: m.trustAcl,
      })),
      [],
    )
    .accountsPartial({
      template: pda,
      admin,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();

  console.log(
    `[ship]  ${label.padEnd(8)} registered at ${pda.toBase58()} (${modules.length} modules)`,
  );
  return { pda, created: true };
}

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const factory = anchor.workspace.aeqiFactory as Program<AeqiFactory>;
  const role = anchor.workspace.aeqiRole as Program<AeqiRole>;
  const token = anchor.workspace.aeqiToken as Program<AeqiToken>;
  const governance = anchor.workspace.aeqiGovernance as Program<AeqiGovernance>;
  const treasury = anchor.workspace.aeqiTreasury as Program<AeqiTreasury>;
  const vesting = anchor.workspace.aeqiVesting as Program<AeqiVesting>;

  console.log(`RPC:    ${provider.connection.rpcEndpoint}`);
  console.log(`Wallet: ${provider.wallet.publicKey.toBase58()}\n`);

  await ensureTemplate(
    factory,
    BASIC_ID,
    "BASIC",
    [
      { moduleId: MODULE_ROLE, programId: role.programId, trustAcl: FULL_ACL },
      { moduleId: MODULE_TOKEN, programId: token.programId, trustAcl: FULL_ACL },
      { moduleId: MODULE_GOV, programId: governance.programId, trustAcl: FULL_ACL },
    ],
    provider.wallet.publicKey,
  );

  await ensureTemplate(
    factory,
    VENTURE_ID,
    "VENTURE",
    [
      { moduleId: MODULE_ROLE, programId: role.programId, trustAcl: FULL_ACL },
      { moduleId: MODULE_TOKEN, programId: token.programId, trustAcl: FULL_ACL },
      { moduleId: MODULE_GOV, programId: governance.programId, trustAcl: FULL_ACL },
      { moduleId: MODULE_TREASURY, programId: treasury.programId, trustAcl: FULL_ACL },
      { moduleId: MODULE_VESTING, programId: vesting.programId, trustAcl: FULL_ACL },
    ],
    provider.wallet.publicKey,
  );

  console.log("\nDone. Templates resolvable at:");
  const [basicPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("template"), Buffer.from(BASIC_ID)],
    factory.programId,
  );
  const [venturePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("template"), Buffer.from(VENTURE_ID)],
    factory.programId,
  );
  console.log(`  BASIC   ${basicPda.toBase58()}`);
  console.log(`  VENTURE ${venturePda.toBase58()}`);
}

main().then(
  () => process.exit(0),
  (err) => {
    console.error(err);
    process.exit(1);
  },
);
