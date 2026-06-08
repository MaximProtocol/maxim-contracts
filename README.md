# Maxim Protocol: On-Chain Contracts

Solana / Anchor program that forms the settlement and audit layer for the Maxim Protocol payment gateway. The program handles agent wallet registration, spend policy enforcement, USDC settlement via SPL Token CPI, and immutable on-chain payment ledger creation.

---

## Architecture

Maxim Protocol abstracts two machine payment standards behind a single on-chain interface:

| Protocol | Standard | Handshake |
|----------|----------|-----------|
| **x402** | HTTP 402-native, Coinbase | Embedded in the HTTP request/response cycle |
| **MPP** | IETF draft, Stripe + Tempo | Off-chain handshake; mirrored on-chain for auditability |

Regardless of which upstream protocol executed, every payment produces a `PaymentRecord` on Solana, making the ledger protocol-agnostic and independently auditable.

Policy enforcement runs at two layers:

1. **Gateway (primary)**: all payments pass through the gateway's policy engine before the program is invoked. Rejections here incur no on-chain fees.
2. **On-chain (secondary)**: payments at or above `high_value_threshold` (default 10 USDC) are additionally enforced by the program. This boundary holds even if the gateway were bypassed.

---

## Program Accounts

### `AgentWallet`

PDA seeds: `[b"agent_wallet", agent_id.as_bytes()]`

The on-chain registry entry for a single agent. Owns a USDC associated token account (ATA) that is the source of all settlements. Non-custodial, so only a valid Ed25519 signature from the registered `owner` keypair can authorise a payment instruction.

| Field | Type | Description |
|-------|------|-------------|
| `agent_id` | `String` (max 64 bytes) | Canonical agent identifier; also the PDA seed |
| `owner` | `Pubkey` | Authorised signer, held by the Maxim Protocol KMS |
| `usdc_token_account` | `Pubkey` | The USDC ATA owned by this PDA |
| `daily_spend` | `u64` | Cumulative USDC spend in the current 24-hour window |
| `daily_window_start` | `i64` | Unix timestamp of the current window start |
| `payment_sequence` | `u64` | Monotonic counter used as `PaymentRecord` PDA seed |
| `total_payments` | `u64` | Lifetime payment and violation record count |
| `total_volume` | `u64` | Lifetime USDC successfully settled |
| `is_active` | `bool` | When `false`, all `settle_payment` calls are rejected |

### `SpendPolicy`

PDA seeds: `[b"spend_policy", agent_wallet.key()]`

Governs the agent wallet's on-chain payment behaviour. Created with `init_if_needed` so the same instruction handles initial setup and subsequent updates.

| Field | Type | Description |
|-------|------|-------------|
| `daily_budget` | `u64` | Max USDC per 24-hour window (6-decimal). Zero = unlimited |
| `per_call_limit` | `u64` | Max USDC per payment call. Zero = unlimited |
| `rate_limit_calls` | `u32` | Max calls within `rate_limit_window_secs`. Zero = unlimited |
| `rate_limit_window_secs` | `u32` | Rate-limit window duration in seconds |
| `high_value_threshold` | `u64` | Min amount that triggers on-chain enforcement. Default: 10 USDC |
| `allowed_domain_hashes` | `Vec<[u8; 32]>` | SHA-256 hashes of permitted hostnames. Empty = all allowed |
| `blocked_domain_hashes` | `Vec<[u8; 32]>` | SHA-256 hashes of blocked hostnames. Evaluated before allowlist |

Domain hashes are computed as `SHA256(bare_hostname)`, for example `SHA256("api.dune.com")`. Full URLs are stored encrypted in the off-chain database; only hashes appear on-chain.

### `PaymentRecord`

PDA seeds: `[b"payment_record", agent_wallet.key(), sequence.to_le_bytes()]`

Immutable, append-only ledger entry created for every payment event, including both successful settlements and policy violations. Accounts are never mutated after creation.

| Field | Type | Description |
|-------|------|-------------|
| `agent_wallet` | `Pubkey` | The originating agent wallet |
| `sequence` | `u64` | Monotonic index within this agent's history |
| `endpoint_hash` | `[u8; 32]` | SHA-256 of the full destination URL |
| `payee` | `Pubkey` | Recipient's token account owner. `Pubkey::default()` for violations |
| `amount_usdc` | `u64` | USDC amount (6-decimal fixed-point) |
| `protocol` | `PaymentProtocol` | `X402 = 0`, `Mpp = 1` |
| `settled_at` | `i64` | Unix timestamp of on-chain confirmation |
| `policy_passed` | `bool` | `false` for violation records; no funds moved |
| `parent_payment` | `Option<Pubkey>` | Links to parent record in multi-agent orchestration chains |

---

## Instructions

### `register_agent(agent_id: String)`

Creates an `AgentWallet` PDA and its USDC ATA. The calling keypair is recorded as the authorised owner. Registration is idempotent at the account level, since Anchor's `init` will reject a duplicate `agent_id`.

### `set_spend_policy(params: SetSpendPolicyParams)`

Creates or updates the `SpendPolicy` for an agent wallet. Only the registered owner keypair may call this. Domain hash lists are capped at 32 entries each.

### `settle_payment(sequence: u64, params: SettlePaymentParams)`

Core settlement instruction. Steps executed in order:

1. Validates the wallet is active
2. Resets the daily spend window if 24 hours have elapsed
3. Enforces `SpendPolicy` on-chain when `amount >= high_value_threshold`
4. Transfers USDC from the agent's ATA to the payee's ATA via SPL Token CPI
5. Updates wallet accounting (`daily_spend`, `total_volume`, `payment_sequence`)
6. Creates an immutable `PaymentRecord`
7. Emits a `PaymentSettled` event for the Geyser indexer

`SettlePaymentParams` carries two hashes: `endpoint_hash` (`SHA256(full_url)`) stored in the ledger, and `domain_hash` (`SHA256(bare_hostname)`) used for policy evaluation. These are computed separately by the gateway.

### `record_policy_violation(sequence, params, violation_reason)`

Records a rejected payment as a `PaymentRecord` with `policy_passed = false`. No USDC is transferred. Used by the gateway to create a permanent, tamper-evident audit trail of policy breaches without relying on off-chain log infrastructure.

### `deactivate_agent` / `reactivate_agent`

Toggle `AgentWallet.is_active`. While inactive, all `settle_payment` calls targeting the wallet are rejected by the program. USDC in the ATA remains accessible via standard SPL Token operations. Both instructions emit an `AgentStatusChanged` event.

---

## Events

All state-changing instructions emit events consumed by the Maxim Protocol dashboard indexer via the Solana Geyser plugin interface.

| Event | Emitted by |
|-------|-----------|
| `PaymentSettled` | `settle_payment` |
| `PolicyViolationRecorded` | `record_policy_violation` |
| `AgentStatusChanged` | `deactivate_agent`, `reactivate_agent` |

---

## Multi-Agent Payment Chains

When an orchestrating agent delegates to sub-agents, each sub-agent payment sets `parent_payment` to the orchestrator's `PaymentRecord` PDA. Starting from any root record, the complete cost of a multi-agent workflow can be reconstructed on-chain by following the `parent_payment` links.

---

## Development

### Prerequisites

- [Rust](https://rustup.rs/) with the `solana` toolchain target
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools) 1.18.17
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) 0.29.0
- [Node.js](https://nodejs.org/) 18+ and Yarn

### Build

```bash
anchor build
```

### Test

Tests require a local validator with the SPL Token program available. Anchor starts one automatically:

```bash
anchor test
```

The test suite covers wallet registration, x402 and MPP settlement, multi-agent parent linking, domain allowlist and blocklist rejection, per-call limit enforcement, zero-amount guard, violation recording, and agent lifecycle management.

### Deploy

```bash
# Localnet
anchor deploy

# Devnet
anchor deploy --provider.cluster devnet
```

Update the program ID in `Anchor.toml` and `declare_id!` after running `anchor keys sync`.

---

## Network Addresses

| Network | Program ID |
|---------|-----------|
| Localnet | `RaiLwY9u7UMjv7FBHTw4Bh4XDWA5pVFJGsDm8UMXMsw` |
| Devnet | `RaiLwY9u7UMjv7FBHTw4Bh4XDWA5pVFJGsDm8UMXMsw` |
| Mainnet | TBD |

USDC mint on mainnet: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`
