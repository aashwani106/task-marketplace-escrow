# Task Marketplace Protocol

A decentralized task escrow marketplace built on Solana using Anchor.

Creators create tasks with fixed SOL rewards. Workers accept and complete tasks. Funds are secured in program-owned escrow vaults. Disputes can be resolved by arbitrators or mutual agreement.

## Program

| Property | Value |
|---|---|
| Program | `task_marketplace` |
| Framework | Anchor 1.1.2 |
| Cluster | devnet |
| Program ID | `FM6bo4u3EMLxMM5NRappPN3ftNzKd7DV5A3z6XFsBQ87` |
| Instructions | 16 |
| Accounts | 5 |
| Events | 16 |
| Escrow Asset | Native SOL |

## Accounts At A Glance

| Account | PDA Seeds | Purpose |
|---|---|---|
| `CreatorProfile` | `["creator_profile", creator]` | Tracks the creator's task sequence |
| `Task` | `["task", creator, task_number_le_u64]` | Permanent task and lifecycle record |
| `EscrowVault` | `["vault", task]` | Holds escrowed native SOL |
| `WorkerAssignment` | `["worker_assignment", task]` | Optional creator-selected worker |
| `TaskResolution` | `["task_resolution", task]` | Optional dispute-resolution state |

## Architecture

Protocol diagrams are stored in [`docs/`](docs/).

### Available Diagrams

- [Protocol Architecture — Poster](docs/diagrams/architecture.png)
- [Task Lifecycle](docs/task-lifecycle.svg)
- Account Relationships, Escrow Flow, and Dispute Lifecycle are included in the detailed poster.

![Protocol Architecture](docs/diagrams/architecture.png)

## Instructions

| Instruction | Purpose |
|---|---|
| `create_creator_profile` | Initialize a creator profile |
| `create_task` | Create a sequentially numbered task |
| `accept_task` | Accept an open task directly |
| `assign_worker` | Select a worker for an open task |
| `accept_assignment` | Accept a creator assignment |
| `fund_task` | Deposit the fixed reward into escrow |
| `submit_task` | Submit a work reference |
| `pay_task` | Approve submission and pay the worker |
| `cancel_task` | Cancel before funding |
| `refund_task_after_timeout` | Return escrow after the submission timeout |
| `settle_task_after_timeout` | Pay the worker after the review timeout |
| `initialize_task_resolution` | Configure an arbitrator while the task is open |
| `reject_submission` | Reject a submission and open a dispute |
| `resolve_dispute` | Resolve through the configured arbitrator |
| `resolve_dispute_by_agreement` | Resolve with creator and worker signatures |
| `settle_dispute_after_timeout` | Pay the worker after the arbitration timeout |

## Task Lifecycle

```text
Open
 ├─> Assigned ─> Accepted
 ├─> Accepted
 └─> Cancelled

Accepted
 ├─> Funded
 └─> Cancelled

Funded
 ├─> Submitted
 └─> Cancelled       (submission-timeout refund)

Submitted
 ├─> Paid
 └─> Disputed

Disputed
 ├─> Paid            (PayWorker)
 └─> Refunded        (RefundCreator)
```

<!-- ![Task Lifecycle](docs/task-lifecycle.svg) -->

`Paid`, `Refunded`, and `Cancelled` are terminal states.

## Escrow Model

- Rewards are denominated in native SOL.
- Each funded task creates a dedicated program-owned `EscrowVault` PDA.
- `EscrowVault.escrowed_lamports` must equal `Task.reward_amount`.
- The worker receives exactly the recorded escrow liability.
- Vaults close after settlement or refund.
- Rent and unsolicited surplus lamports return to the creator.

## Events

Every event includes `version = 1`.

| Event | Emitted by | Event | Emitted by |
|---|---|---|---|
| `CreatorProfileCreated` | `create_creator_profile` | `TaskCreated` | `create_task` |
| `TaskAccepted` | `accept_task` | `WorkerAssigned` | `assign_worker` |
| `AssignmentAccepted` | `accept_assignment` | `TaskFunded` | `fund_task` |
| `TaskSubmitted` | `submit_task` | `TaskCancelled` | `cancel_task` |
| `TaskPaid` | `pay_task` | `TaskRefundedAfterTimeout` | `refund_task_after_timeout` |
| `TaskSettledAfterTimeout` | `settle_task_after_timeout` | `TaskResolutionInitialized` | `initialize_task_resolution` |
| `SubmissionRejected` | `reject_submission` | `DisputeResolved` | `resolve_dispute` |
| `DisputeResolvedByAgreement` | `resolve_dispute_by_agreement` | `DisputeSettledAfterTimeout` | `settle_dispute_after_timeout` |

## Security Properties

- Canonical PDA derivation and account-substitution protection
- Program-controlled escrow custody
- Escrow liability and rent-solvency validation
- Atomic payout, refund, and vault closure
- State-based replay and double-settlement protection
- Permissionless timeout settlement
- Arbitrator and mutual-agreement dispute resolution
- Upgradeable devnet deployment

## Reference

### Program

- Program ID: `FM6bo4u3EMLxMM5NRappPN3ftNzKd7DV5A3z6XFsBQ87`

### Deployment

- Cluster: `devnet`
- Upgrade Authority: `C9CnYM5j7bkH2JcBXkd4L9aYThLsSDnuDrDyJgg7thJh`

<!-- ### Documentation

- [Detailed Architecture](docs/protocol_architecture.md)
- [Architecture Diagrams](docs/) -->
