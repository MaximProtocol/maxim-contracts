/**
 * Seed 15 on-chain transactions for the maxim_protocol devnet deployment.
 *
 * What it does:
 *   1. Creates a mock USDC mint (deployer is mint authority)
 *   2. Initialises the ProtocolConfig singleton
 *   3. Registers 2 agent wallets and sets their spend policies
 *   4. Funds each agent's ATA with mock USDC
 *   5. Settles 12 payments across both agents (mix of x402 / MPP protocols)
 *   6. Records 3 policy violations for agent-002
 *
 * Run:
 *   npx ts-node scripts/seed.ts
 */

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
  clusterApiUrl,
} from "@solana/web3.js";
import {
  createMint,
  getAssociatedTokenAddressSync,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import * as crypto from "crypto";
import * as fs from "fs";
import * as path from "path";

// ── Config ────────────────────────────────────────────────────────────────────

const PROGRAM_ID = new PublicKey("337JEF6PSMGSPwBSseMHFa95YxLACnJeehA5brVAgzKh");
const KEYS_DIR = path.resolve(__dirname, "../.keys");
const deployer = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync(path.join(KEYS_DIR, "deployer.json"), "utf8")))
);
const connection = new Connection(clusterApiUrl("devnet"), "confirmed");

// Unique suffix so agent IDs are fresh on every run
const RUN_SUFFIX = Date.now().toString(36);

// ── Anchor discriminator ──────────────────────────────────────────────────────

function disc(name: string): Buffer {
  return Buffer.from(crypto.createHash("sha256").update(`global:${name}`).digest()).slice(0, 8);
}

// ── Borsh encoding helpers ────────────────────────────────────────────────────

const encU64 = (n: bigint): Buffer => {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(n);
  return b;
};
const encU32 = (n: number): Buffer => {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n);
  return b;
};
const encStr = (s: string): Buffer => {
  const bytes = Buffer.from(s, "utf8");
  return Buffer.concat([encU32(bytes.length), bytes]);
};
const encOptPk = (pk: PublicKey | null): Buffer =>
  pk ? Buffer.concat([Buffer.from([1]), pk.toBuffer()]) : Buffer.from([0]);
const encVecHash = (hs: Buffer[]): Buffer =>
  Buffer.concat([encU32(hs.length), ...hs]);
const encHash = (): Buffer => crypto.randomBytes(32);

// ── PDA derivations ───────────────────────────────────────────────────────────

const protocolConfigPda = (): [PublicKey, number] =>
  PublicKey.findProgramAddressSync([Buffer.from("protocol_config")], PROGRAM_ID);

const agentWalletPda = (agentId: string): [PublicKey, number] =>
  PublicKey.findProgramAddressSync(
    [Buffer.from("agent_wallet"), Buffer.from(agentId)],
    PROGRAM_ID
  );

const spendPolicyPda = (walletPk: PublicKey): [PublicKey, number] =>
  PublicKey.findProgramAddressSync(
    [Buffer.from("spend_policy"), walletPk.toBuffer()],
    PROGRAM_ID
  );

const paymentRecordPda = (walletPk: PublicKey, seq: bigint): [PublicKey, number] => {
  const seqBuf = Buffer.alloc(8);
  seqBuf.writeBigUInt64LE(seq);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("payment_record"), walletPk.toBuffer(), seqBuf],
    PROGRAM_ID
  );
};

// ── Transaction helper ────────────────────────────────────────────────────────

async function send(
  ixs: TransactionInstruction[],
  signers: Keypair[] = [deployer]
): Promise<string> {
  const tx = new Transaction().add(...ixs);
  return sendAndConfirmTransaction(connection, tx, signers, { commitment: "confirmed" });
}

// ── Instruction builders ──────────────────────────────────────────────────────

function initProtocolIx(admin: PublicKey): TransactionInstruction {
  const [configPda] = protocolConfigPda();
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: configPda, isSigner: false, isWritable: true },
      { pubkey: deployer.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([disc("init_protocol"), admin.toBuffer()]),
  });
}

function registerAgentIx(
  agentId: string,
  usdcMint: PublicKey,
  owner: PublicKey
): TransactionInstruction {
  const [walletPda] = agentWalletPda(agentId);
  const agentAta = getAssociatedTokenAddressSync(usdcMint, walletPda, true);
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: agentAta, isSigner: false, isWritable: true },
      { pubkey: usdcMint, isSigner: false, isWritable: false },
      { pubkey: owner, isSigner: true, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([disc("register_agent"), encStr(agentId)]),
  });
}

interface SpendPolicyParams {
  dailyBudget: bigint;
  weeklyBudget: bigint;
  perCallLimit: bigint;
  rateLimitCalls: number;
  rateLimitWindowSecs: number;
  highValueThreshold: bigint;
  allowedDomainHashes: Buffer[];
  blockedDomainHashes: Buffer[];
}

function setSpendPolicyIx(
  agentId: string,
  params: SpendPolicyParams,
  owner: PublicKey
): TransactionInstruction {
  const [walletPda] = agentWalletPda(agentId);
  const [policyPda] = spendPolicyPda(walletPda);
  const data = Buffer.concat([
    disc("set_spend_policy"),
    encU64(params.dailyBudget),
    encU64(params.weeklyBudget),
    encU64(params.perCallLimit),
    encU32(params.rateLimitCalls),
    encU32(params.rateLimitWindowSecs),
    encU64(params.highValueThreshold),
    encVecHash(params.allowedDomainHashes),
    encVecHash(params.blockedDomainHashes),
  ]);
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: policyPda, isSigner: false, isWritable: true },
      { pubkey: walletPda, isSigner: false, isWritable: false },
      { pubkey: owner, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

interface PaymentParams {
  agentId: string;
  endpointHash: Buffer;
  domainHash: Buffer;
  amountUsdc: bigint;
  protocol: number; // 0 = x402, 1 = MPP
  parentPayment: PublicKey | null;
}

function settlePaymentIx(
  sequence: bigint,
  params: PaymentParams,
  usdcMint: PublicKey,
  payeeTokenAccount: PublicKey,
  owner: PublicKey
): TransactionInstruction {
  const [walletPda] = agentWalletPda(params.agentId);
  const [policyPda] = spendPolicyPda(walletPda);
  const [recordPda] = paymentRecordPda(walletPda, sequence);
  const agentAta = getAssociatedTokenAddressSync(usdcMint, walletPda, true);

  const paramBytes = Buffer.concat([
    encStr(params.agentId),
    params.endpointHash,
    params.domainHash,
    encU64(params.amountUsdc),
    Buffer.from([params.protocol]),
    encOptPk(params.parentPayment),
  ]);

  const data = Buffer.concat([disc("settle_payment"), encU64(sequence), paramBytes]);

  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: recordPda, isSigner: false, isWritable: true },
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: policyPda, isSigner: false, isWritable: true },
      { pubkey: agentAta, isSigner: false, isWritable: true },
      { pubkey: payeeTokenAccount, isSigner: false, isWritable: true },
      { pubkey: usdcMint, isSigner: false, isWritable: false },
      { pubkey: owner, isSigner: true, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

function recordViolationIx(
  sequence: bigint,
  params: PaymentParams,
  violationReason: number,
  owner: PublicKey
): TransactionInstruction {
  const [walletPda] = agentWalletPda(params.agentId);
  const [recordPda] = paymentRecordPda(walletPda, sequence);

  const paramBytes = Buffer.concat([
    encStr(params.agentId),
    params.endpointHash,
    params.domainHash,
    encU64(params.amountUsdc),
    Buffer.from([params.protocol]),
    encOptPk(params.parentPayment),
  ]);

  const data = Buffer.concat([
    disc("record_policy_violation"),
    encU64(sequence),
    paramBytes,
    Buffer.from([violationReason]),
  ]);

  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: recordPda, isSigner: false, isWritable: true },
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: owner, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

// ── Seed payments table ───────────────────────────────────────────────────────

const AGENT_1 = `agent-001-${RUN_SUFFIX}`;
const AGENT_2 = `agent-002-${RUN_SUFFIX}`;

// 3 settle_payment + 1 record_policy_violation = 4 seeded txns
const PAYMENTS: Array<{
  agentId: string;
  amountUsdc: bigint;  // 6-decimal (1 USDC = 1_000_000)
  protocol: number;
  endpoint: string;
  domain: string;
}> = [
  { agentId: AGENT_1, amountUsdc: 6_000_000n,  protocol: 0, endpoint: "https://api.dune.com/queries/run",           domain: "api.dune.com" },
  { agentId: AGENT_1, amountUsdc: 3_500_000n,  protocol: 1, endpoint: "https://api.perplexity.ai/chat/completions", domain: "api.perplexity.ai" },
  { agentId: AGENT_2, amountUsdc: 9_000_000n,  protocol: 0, endpoint: "https://api.anthropic.com/v1/messages",      domain: "api.anthropic.com" },
];

// 1 policy violation
const VIOLATIONS: Array<{
  agentId: string;
  amountUsdc: bigint;
  protocol: number;
  endpoint: string;
  domain: string;
  reason: number;
}> = [
  { agentId: AGENT_2, amountUsdc: 60_000_000n, protocol: 0, endpoint: "https://api.openai.com/v1/images", domain: "api.openai.com", reason: 5 /* PerCallLimitExceeded */ },
];

// ── Main ──────────────────────────────────────────────────────────────────────

async function main() {
  console.log(`Deployer: ${deployer.publicKey.toBase58()}`);
  const balance = await connection.getBalance(deployer.publicKey);
  console.log(`Balance:  ${(balance / 1e9).toFixed(2)} SOL\n`);

  // 1 ── Mock USDC mint
  process.stdout.write("Creating mock USDC mint … ");
  const usdcMint = await createMint(
    connection,
    deployer,
    deployer.publicKey, // mint authority
    null,               // freeze authority
    6                   // 6 decimals (matches USDC)
  );
  console.log(`✓  ${usdcMint.toBase58()}`);

  // 2 ── Create deployer's receiving ATA (payee for all payments)
  process.stdout.write("Creating deployer USDC ATA … ");
  const deployerAta = await getOrCreateAssociatedTokenAccount(
    connection, deployer, usdcMint, deployer.publicKey
  );
  console.log(`✓  ${deployerAta.address.toBase58()}`);

  // 3 ── init_protocol (singleton — skip if already exists)
  process.stdout.write("init_protocol … ");
  const [configPda] = protocolConfigPda();
  const existing = await connection.getAccountInfo(configPda);
  if (existing) {
    console.log("(already initialised, skipping)");
  } else {
    const initSig = await send([initProtocolIx(deployer.publicKey)]);
    console.log(`✓  ${initSig}`);
  }

  // 4 ── Register agents + set spend policies + fund ATAs
  const agents = [AGENT_1, AGENT_2];
  const agentFundAmounts: bigint[] = [30_000_000n, 30_000_000n]; // 30 USDC each

  for (let i = 0; i < agents.length; i++) {
    const agentId = agents[i];
    process.stdout.write(`register_agent(${agentId}) … `);
    const regSig = await send([registerAgentIx(agentId, usdcMint, deployer.publicKey)]);
    console.log(`✓  ${regSig}`);

    process.stdout.write(`set_spend_policy(${agentId}) … `);
    const policySig = await send([
      setSpendPolicyIx(
        agentId,
        {
          dailyBudget: 50_000_000n,     // 50 USDC
          weeklyBudget: 200_000_000n,   // 200 USDC
          perCallLimit: 50_000_000n,    // 50 USDC per call
          rateLimitCalls: 100,
          rateLimitWindowSecs: 3600,
          highValueThreshold: 10_000_000n, // 10 USDC — enforce on-chain above this
          allowedDomainHashes: [],
          blockedDomainHashes: [],
        },
        deployer.publicKey
      ),
    ]);
    console.log(`✓  ${policySig}`);

    // Fund agent ATA
    const [walletPda] = agentWalletPda(agentId);
    const agentAta = getAssociatedTokenAddressSync(usdcMint, walletPda, true);
    process.stdout.write(`Funding ${agentId} ATA with ${Number(agentFundAmounts[i]) / 1e6} USDC … `);
    await mintTo(connection, deployer, usdcMint, agentAta, deployer, agentFundAmounts[i]);
    console.log("✓");
  }

  // 5 ── 15 seeded transactions: 12 settle_payment + 3 record_policy_violation
  console.log("\n── Seeding 4 on-chain transactions ──────────────────────");

  // Track per-agent sequence numbers locally
  const seqMap: Record<string, bigint> = { [AGENT_1]: 0n, [AGENT_2]: 0n };

  let txCount = 0;

  // 12 successful payments
  for (const payment of PAYMENTS) {
    const seq = seqMap[payment.agentId];
    const endpointHash = Buffer.from(
      crypto.createHash("sha256").update(payment.endpoint).digest()
    );
    const domainHash = Buffer.from(
      crypto.createHash("sha256").update(payment.domain).digest()
    );

    const params: PaymentParams = {
      agentId: payment.agentId,
      endpointHash,
      domainHash,
      amountUsdc: payment.amountUsdc,
      protocol: payment.protocol,
      parentPayment: null,
    };

    process.stdout.write(
      `[${++txCount}/4] settle_payment(${payment.agentId}, seq=${seq}, ` +
      `${Number(payment.amountUsdc) / 1e6} USDC, ${payment.protocol === 0 ? "x402" : "MPP "}) … `
    );

    const sig = await send([
      settlePaymentIx(seq, params, usdcMint, deployerAta.address, deployer.publicKey),
    ]);
    console.log(`✓  ${sig.slice(0, 20)}…`);
    seqMap[payment.agentId]++;
  }

  // 3 policy violations (agent-002)
  for (const v of VIOLATIONS) {
    const seq = seqMap[v.agentId];
    const endpointHash = Buffer.from(
      crypto.createHash("sha256").update(v.endpoint).digest()
    );
    const domainHash = Buffer.from(
      crypto.createHash("sha256").update(v.domain).digest()
    );

    const params: PaymentParams = {
      agentId: v.agentId,
      endpointHash,
      domainHash,
      amountUsdc: v.amountUsdc,
      protocol: v.protocol,
      parentPayment: null,
    };

    process.stdout.write(
      `[${++txCount}/4] record_violation(${v.agentId}, seq=${seq}, ` +
      `${Number(v.amountUsdc) / 1e6} USDC, reason=${v.reason}) … `
    );

    const sig = await send([
      recordViolationIx(seq, params, v.reason, deployer.publicKey),
    ]);
    console.log(`✓  ${sig.slice(0, 20)}…`);
    seqMap[v.agentId]++;
  }

  console.log("\n✅  Done! 15 transactions seeded on devnet.");
  console.log(`   Mock USDC mint: ${usdcMint.toBase58()}`);
  console.log(`   Program:        ${PROGRAM_ID.toBase58()}`);
  console.log(`   Explorer:       https://explorer.solana.com/address/${PROGRAM_ID.toBase58()}?cluster=devnet`);
}

main().catch((err) => {
  console.error("\n❌", err);
  process.exit(1);
});
