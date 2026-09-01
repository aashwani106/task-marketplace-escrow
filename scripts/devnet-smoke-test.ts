import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  AnchorProvider,
  type Idl,
  Program,
  Wallet,
} from "@anchor-lang/core";
import BN from "bn.js";
import {
  clusterApiUrl,
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  type Signer,
  SystemProgram,
  type Transaction,
} from "@solana/web3.js";

import type { TaskMarketplace } from "../target/types/task_marketplace.js";

const PROGRAM_ID = new PublicKey(
  "FM6bo4u3EMLxMM5NRappPN3ftNzKd7DV5A3z6XFsBQ87",
);
const DEVNET_RPC_URL = clusterApiUrl("devnet");
const COMMITMENT = "confirmed" as const;

const CREATOR_PROFILE_SEED = Buffer.from("creator_profile");
const TASK_SEED = Buffer.from("task");
const VAULT_SEED = Buffer.from("vault");
const WORKER_ASSIGNMENT_SEED = Buffer.from("worker_assignment");

const REWARD_LAMPORTS = new BN(1_000_000);
const MIN_CREATOR_BALANCE_LAMPORTS = 25_000_000;

type CreatorProfileAccount = {
  taskCount: BN;
  creator: PublicKey;
};

type TaskAccount = {
  taskNumber: BN;
  creator: PublicKey;
  worker: PublicKey | null;
  title: string;
  description: string;
  rewardAmount: BN;
  status: Record<string, unknown>;
  submissionReference: string | null;
  fundedAt: BN | null;
  submissionDeadline: BN | null;
  reviewDeadline: BN | null;
};

type AnchorMethodBuilder = {
  transaction(): Promise<Transaction>;
  rpc(options?: { commitment?: typeof COMMITMENT }): Promise<string>;
};

function taskNumberSeed(taskNumber: BN): Buffer {
  return taskNumber.toArrayLike(Buffer, "le", 8);
}

function statusName(status: Record<string, unknown>): string {
  const variants = Object.keys(status);
  assert.equal(variants.length, 1, `Invalid task status: ${JSON.stringify(status)}`);
  return variants[0];
}

function jsonReplacer(_key: string, value: unknown): unknown {
  if (value instanceof PublicKey) {
    return value.toBase58();
  }
  if (BN.isBN(value)) {
    return (value as BN).toString(10);
  }
  if (typeof value === "bigint") {
    return value.toString(10);
  }
  return value;
}

function printPda(label: string, address: PublicKey, bump: number): void {
  console.log(`${label}: ${address.toBase58()} (bump ${bump})`);
}

async function fetchAndAssertTask(
  program: Program<TaskMarketplace>,
  taskAddress: PublicKey,
  transition: string,
  expectedStatus: string,
): Promise<TaskAccount> {
  const task = (await program.account.task.fetch(taskAddress)) as TaskAccount;
  const actualStatus = statusName(task.status);
  console.log(
    `${transition} task state:\n${JSON.stringify(task, jsonReplacer, 2)}`,
  );
  assert.equal(
    actualStatus,
    expectedStatus,
    `${transition}: expected status ${expectedStatus}, received ${actualStatus}`,
  );
  return task;
}

async function loadKeypair(path: string): Promise<Keypair> {
  const raw = JSON.parse(await readFile(path, "utf8")) as unknown;
  assert.ok(Array.isArray(raw), `Wallet file is not a byte array: ${path}`);
  assert.equal(raw.length, 64, `Wallet file must contain 64 bytes: ${path}`);
  assert.ok(
    raw.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255),
    `Wallet file contains an invalid secret-key byte: ${path}`,
  );
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

async function main(): Promise<void> {
  const scriptDirectory = dirname(fileURLToPath(import.meta.url));
  const repositoryRoot = resolve(scriptDirectory, "..");
  const idlPath = resolve(repositoryRoot, "target/idl/task_marketplace.json");
  const walletPath = resolve(homedir(), ".config/solana/id.json");

  const idl = JSON.parse(await readFile(idlPath, "utf8")) as TaskMarketplace &
    Idl;
  assert.equal(
    idl.address,
    PROGRAM_ID.toBase58(),
    `IDL program ID ${idl.address} does not match ${PROGRAM_ID.toBase58()}`,
  );

  const creator = await loadKeypair(walletPath);
  const worker = Keypair.generate();
  const connection = new Connection(DEVNET_RPC_URL, COMMITMENT);
  const provider = new AnchorProvider(connection, new Wallet(creator), {
    commitment: COMMITMENT,
    preflightCommitment: COMMITMENT,
  });
  const program = new Program<TaskMarketplace>(idl, provider);

  async function simulateAndSend(
    label: string,
    builder: AnchorMethodBuilder,
    additionalSigners: Signer[] = [],
  ): Promise<string> {
    const transaction = await builder.transaction();
    const simulation = await connection.simulateTransaction(transaction, [
      creator,
      ...additionalSigners,
    ]);
    assert.equal(
      simulation.value.err,
      null,
      `${label} simulation failed: ${JSON.stringify(simulation.value.err)}\n${
        simulation.value.logs?.join("\n") ?? "No simulation logs"
      }`,
    );
    console.log(`${label} simulation: ok`);

    const signature = await builder.rpc({ commitment: COMMITMENT });
    console.log(`${label} signature: ${signature}`);
    return signature;
  }

  assert.ok(program.programId.equals(PROGRAM_ID), "Anchor client program ID mismatch");
  const programAccount = await connection.getAccountInfo(PROGRAM_ID, COMMITMENT);
  assert.ok(programAccount, `Program ${PROGRAM_ID.toBase58()} is not deployed on devnet`);
  assert.ok(programAccount.executable, `Program ${PROGRAM_ID.toBase58()} is not executable`);

  const creatorBalance = await connection.getBalance(creator.publicKey, COMMITMENT);
  assert.ok(
    creatorBalance >= MIN_CREATOR_BALANCE_LAMPORTS,
    `Creator ${creator.publicKey.toBase58()} has ${creatorBalance / LAMPORTS_PER_SOL} SOL; ` +
      `at least ${MIN_CREATOR_BALANCE_LAMPORTS / LAMPORTS_PER_SOL} SOL is required`,
  );

  console.log(`RPC: ${DEVNET_RPC_URL}`);
  console.log(`Program: ${PROGRAM_ID.toBase58()}`);
  console.log(`Creator: ${creator.publicKey.toBase58()}`);
  console.log(`Worker: ${worker.publicKey.toBase58()}`);

  const [creatorProfile, creatorProfileBump] = PublicKey.findProgramAddressSync(
    [CREATOR_PROFILE_SEED, creator.publicKey.toBuffer()],
    PROGRAM_ID,
  );
  printPda("CreatorProfile PDA", creatorProfile, creatorProfileBump);

  if ((await connection.getAccountInfo(creatorProfile, COMMITMENT)) === null) {
    await simulateAndSend(
      "create_creator_profile",
      program.methods
        .createCreatorProfile()
        .accountsStrict({
          creator: creator.publicKey,
          creatorProfile,
          systemProgram: SystemProgram.programId,
        }),
    );
  } else {
    console.log("CreatorProfile already exists; reusing it for a repeatable smoke test.");
  }

  const profile = (await program.account.creatorProfile.fetch(
    creatorProfile,
  )) as CreatorProfileAccount;
  assert.ok(profile.creator.equals(creator.publicKey), "CreatorProfile creator mismatch");
  console.log(`CreatorProfile state:\n${JSON.stringify(profile, jsonReplacer, 2)}`);

  const taskNumber = profile.taskCount.addn(1);
  const [task, taskBump] = PublicKey.findProgramAddressSync(
    [TASK_SEED, creator.publicKey.toBuffer(), taskNumberSeed(taskNumber)],
    PROGRAM_ID,
  );
  const [workerAssignment, workerAssignmentBump] =
    PublicKey.findProgramAddressSync(
      [WORKER_ASSIGNMENT_SEED, task.toBuffer()],
      PROGRAM_ID,
    );
  const [escrowVault, escrowVaultBump] = PublicKey.findProgramAddressSync(
    [VAULT_SEED, task.toBuffer()],
    PROGRAM_ID,
  );

  printPda("Task PDA", task, taskBump);
  printPda("WorkerAssignment PDA", workerAssignment, workerAssignmentBump);
  printPda("EscrowVault PDA", escrowVault, escrowVaultBump);

  const uniqueSuffix = `${taskNumber.toString(10)}-${Date.now()}`;
  await simulateAndSend(
    "create_task",
    program.methods
      .createTask(
        taskNumber,
        `Devnet smoke task ${uniqueSuffix}`,
        "End-to-end devnet smoke test for the task marketplace.",
        REWARD_LAMPORTS,
      )
      .accountsStrict({
        creator: creator.publicKey,
        creatorProfile,
        task,
        systemProgram: SystemProgram.programId,
      }),
  );
  await fetchAndAssertTask(program, task, "create_task", "open");

  await simulateAndSend(
    "assign_worker",
    program.methods
      .assignWorker(worker.publicKey)
      .accountsStrict({
        creator: creator.publicKey,
        task,
        workerAssignment,
        systemProgram: SystemProgram.programId,
      }),
  );
  await fetchAndAssertTask(program, task, "assign_worker", "assigned");

  await simulateAndSend(
    "accept_assignment",
    program.methods
      .acceptAssignment()
      .accountsStrict({
        worker: worker.publicKey,
        task,
        workerAssignment,
      })
      .signers([worker]),
    [worker],
  );
  const acceptedTask = await fetchAndAssertTask(
    program,
    task,
    "accept_assignment",
    "accepted",
  );
  assert.ok(acceptedTask.worker?.equals(worker.publicKey), "Stored worker mismatch");

  await simulateAndSend(
    "fund_task",
    program.methods
      .fundTask()
      .accountsStrict({
        creator: creator.publicKey,
        task,
        escrowVault,
        systemProgram: SystemProgram.programId,
      }),
  );
  const fundedTask = await fetchAndAssertTask(
    program,
    task,
    "fund_task",
    "funded",
  );
  assert.equal(
    fundedTask.rewardAmount.toString(10),
    REWARD_LAMPORTS.toString(10),
    "Funded reward mismatch",
  );
  assert.ok(await connection.getAccountInfo(escrowVault, COMMITMENT), "Vault was not created");

  await simulateAndSend(
    "submit_task",
    program.methods
      .submitTask(`ipfs://devnet-smoke/${uniqueSuffix}`)
      .accountsStrict({ worker: worker.publicKey, task })
      .signers([worker]),
    [worker],
  );
  await fetchAndAssertTask(program, task, "submit_task", "submitted");

  const workerBalanceBefore = await connection.getBalance(worker.publicKey, COMMITMENT);
  await simulateAndSend(
    "pay_task",
    program.methods.payTask().accountsStrict({
      creator: creator.publicKey,
      task,
      escrowVault,
      worker: worker.publicKey,
    }),
  );
  await fetchAndAssertTask(program, task, "pay_task", "paid");

  const workerBalanceAfter = await connection.getBalance(worker.publicKey, COMMITMENT);
  assert.equal(
    workerBalanceAfter - workerBalanceBefore,
    REWARD_LAMPORTS.toNumber(),
    "Worker did not receive exactly the escrowed reward",
  );
  assert.equal(
    await connection.getAccountInfo(escrowVault, COMMITMENT),
    null,
    "Escrow vault was not closed after payment",
  );

  console.log(`Worker payout verified: ${REWARD_LAMPORTS.toString(10)} lamports`);
  console.log("Escrow vault closure verified.");
  console.log("Devnet smoke test completed successfully.");
}

main().catch((error: unknown) => {
  console.error("Devnet smoke test failed:", error);
  process.exitCode = 1;
});
