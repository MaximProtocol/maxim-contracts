use anchor_lang::prelude::*;

pub const MAX_AGENT_ID_LEN: usize = 64;
const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_WEEK: i64 = 604_800;

/// On-chain registry entry for a single Maxim Protocol agent.
///
/// Each agent registered through the SDK maps to exactly one AgentWallet PDA,
/// derived from `[b"agent_wallet", agent_id.as_bytes()]`. The PDA owns a USDC
/// associated token account (ATA) via the SPL Token program, making it the
/// on-chain settlement source for all payments routed through the gateway.
///
/// Wallets are non-custodial: only a valid Ed25519 signature from the registered
/// `owner` keypair can authorise a payment instruction. The Maxim Protocol gateway
/// holds this keypair in its KMS and signs on behalf of the agent, but cannot
/// move funds without it.
///
/// Two independent suspension flags exist with different trust levels:
/// - `is_active`: owner-controlled. Deactivation/reactivation via `deactivate_agent` /
///   `reactivate_agent`. Used for intentional pauses (e.g. billing hold).
/// - `is_frozen`: protocol-admin-controlled. Set/cleared by the `ProtocolConfig` admin
///   via `freeze_agent` / `unfreeze_agent`. Reserved for security incidents. Frozen
///   wallets cannot process payments but the owner can still withdraw funds at any time.
#[account]
pub struct AgentWallet {
    /// Canonical agent identifier matching the SDK `agentId` field.
    /// Used as a PDA seed, so it uniquely scopes this wallet on-chain.
    pub agent_id: String,

    /// The Ed25519 keypair authorised to sign payment and policy instructions
    /// on behalf of this agent. Held by the Maxim Protocol KMS.
    pub owner: Pubkey,

    /// The SPL Token USDC associated token account (ATA) owned by this PDA.
    /// All incoming and outgoing USDC flows through this account.
    pub usdc_token_account: Pubkey,

    /// Cumulative USDC spend in the current 24-hour accounting window.
    /// Stored in 6-decimal fixed-point (1 USDC = 1_000_000 units).
    /// Resets to zero at the start of each new window.
    pub daily_spend: u64,

    /// Unix timestamp (seconds) marking the start of the current daily window.
    pub daily_window_start: i64,

    /// Cumulative USDC spend in the current 7-day accounting window.
    /// 6-decimal fixed-point. Resets at the start of each new 7-day window.
    /// Governed by `SpendPolicy.weekly_budget`.
    pub weekly_spend: u64,

    /// Unix timestamp (seconds) marking the start of the current weekly window.
    pub weekly_window_start: i64,

    /// Monotonically increasing counter. Each settled payment or recorded
    /// violation increments this value and uses the pre-increment value as
    /// part of the `PaymentRecord` PDA seed, ensuring uniqueness.
    pub payment_sequence: u64,

    /// Total number of payments settled or violations recorded over the
    /// lifetime of this wallet. Includes both successful and rejected entries.
    pub total_payments: u64,

    /// Total USDC transferred from this wallet over its lifetime.
    /// 6-decimal fixed-point. Reflects successfully settled payments only.
    pub total_volume: u64,

    /// When `false`, all `settle_payment` instructions targeting this wallet
    /// are rejected. Funds remain in the ATA and are accessible via direct
    /// SPL Token operations or `withdraw_funds`.
    pub is_active: bool,

    /// When `true`, the wallet has been frozen by the Maxim Protocol security
    /// admin (`ProtocolConfig.admin`). Payment settlement is blocked; fund
    /// withdrawal by the owner is still permitted.
    pub is_frozen: bool,

    /// Canonical PDA bump seed, cached to avoid recomputation in CPI signing
    /// contexts where the AgentWallet PDA acts as the token account authority.
    pub bump: u8,
}

impl AgentWallet {
    pub const LEN: usize = 8                   // Anchor discriminator
        + 4 + MAX_AGENT_ID_LEN                  // agent_id: String (4-byte len prefix + max bytes)
        + 32                                    // owner
        + 32                                    // usdc_token_account
        + 8                                     // daily_spend
        + 8                                     // daily_window_start
        + 8                                     // weekly_spend
        + 8                                     // weekly_window_start
        + 8                                     // payment_sequence
        + 8                                     // total_payments
        + 8                                     // total_volume
        + 1                                     // is_active
        + 1                                     // is_frozen
        + 1;                                    // bump

    /// Resets the daily spend accumulator if the 24-hour window has elapsed.
    /// Called at the start of every `settle_payment` to ensure budget checks
    /// operate on the current window.
    pub fn reset_daily_window_if_elapsed(&mut self, now: i64) {
        if now >= self.daily_window_start + SECONDS_PER_DAY {
            self.daily_spend = 0;
            self.daily_window_start = now;
        }
    }

    /// Resets the weekly spend accumulator if the 7-day window has elapsed.
    /// Called alongside `reset_daily_window_if_elapsed` in `settle_payment`.
    pub fn reset_weekly_window_if_elapsed(&mut self, now: i64) {
        if now >= self.weekly_window_start + SECONDS_PER_WEEK {
            self.weekly_spend = 0;
            self.weekly_window_start = now;
        }
    }
}
