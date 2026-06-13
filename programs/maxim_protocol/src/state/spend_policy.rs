use anchor_lang::prelude::*;

/// Maximum number of domain hashes that a single allow or block list can hold.
pub const MAX_DOMAIN_ENTRIES: usize = 32;

/// On-chain spend policy governing an AgentWallet's payment behaviour.
///
/// Policies are enforced at two layers:
///
/// 1. **Gateway layer (primary):** All payment requests pass through the gateway's
///    policy engine before reaching the protocol handlers. Rejections at this layer
///    are instant and incur no on-chain fees.
///
/// 2. **On-chain layer (this account):** Payments at or above `high_value_threshold`
///    are additionally enforced by the program. Even if the gateway were bypassed,
///    the on-chain checks prevent unauthorised high-value transfers. The default
///    threshold is 10 USDC; enterprise accounts can lower it to zero to enforce
///    all payments on-chain.
///
/// Domain lists store SHA-256 hashes of domain strings (e.g. `SHA256("api.dune.com")`).
/// Full URLs are stored encrypted in the off-chain database; only hashes appear on-chain.
#[account]
pub struct SpendPolicy {
    /// The AgentWallet this policy governs.
    pub agent_wallet: Pubkey,

    /// Maximum USDC the agent may spend in any 24-hour window.
    /// 6-decimal fixed-point. Zero disables this limit.
    pub daily_budget: u64,

    /// Maximum USDC the agent may spend in any 7-day window.
    /// 6-decimal fixed-point. Zero disables this limit.
    pub weekly_budget: u64,

    /// Maximum USDC the agent may spend in any 30-day window.
    /// 6-decimal fixed-point. Zero disables this limit.
    /// Provides a third accumulator for teams that think in monthly budgets,
    /// complementing the daily and weekly caps.
    pub monthly_budget: u64,

    /// Maximum USDC per individual payment call.
    /// 6-decimal fixed-point. Zero disables this limit.
    pub per_call_limit: u64,

    /// Maximum payment calls permitted within `rate_limit_window_secs`.
    /// Zero disables rate limiting.
    pub rate_limit_calls: u32,

    /// Duration of the rate-limit window in seconds.
    /// Ignored when `rate_limit_calls` is zero.
    pub rate_limit_window_secs: u32,

    /// Number of payment calls made in the current rate-limit window.
    /// Only incremented for payments that trigger on-chain enforcement
    /// (i.e. at or above `high_value_threshold`).
    pub rate_limit_call_count: u32,

    /// Unix timestamp marking the start of the current rate-limit window.
    pub rate_limit_window_start: i64,

    /// Minimum USDC amount (6-decimal fixed-point) that triggers on-chain
    /// policy enforcement. Payments below this value rely solely on the
    /// gateway enforcement layer and do not incur on-chain check overhead.
    ///
    /// Default: 10_000_000 (10.00 USDC). Set to 1 to enforce every payment
    /// on-chain; set to 0 to disable on-chain enforcement entirely.
    pub high_value_threshold: u64,

    /// SHA-256 hashes of permitted destination domain strings.
    /// An empty list permits all destination domains.
    pub allowed_domain_hashes: Vec<[u8; 32]>,

    /// SHA-256 hashes of explicitly blocked destination domain strings.
    /// Evaluated before the allowlist. A domain in both lists is always blocked.
    pub blocked_domain_hashes: Vec<[u8; 32]>,

    /// PDA canonical bump seed.
    pub bump: u8,
}

impl SpendPolicy {
    pub const LEN: usize = 8                               // Anchor discriminator
        + 32                                                // agent_wallet
        + 8                                                 // daily_budget
        + 8                                                 // weekly_budget
        + 8                                                 // monthly_budget
        + 8                                                 // per_call_limit
        + 4                                                 // rate_limit_calls
        + 4                                                 // rate_limit_window_secs
        + 4                                                 // rate_limit_call_count
        + 8                                                 // rate_limit_window_start
        + 8                                                 // high_value_threshold
        + 4 + (MAX_DOMAIN_ENTRIES * 32)                    // allowed_domain_hashes
        + 4 + (MAX_DOMAIN_ENTRIES * 32)                    // blocked_domain_hashes
        + 1;                                                // bump

    /// Returns `true` if `amount` does not exceed the per-call limit.
    /// Always returns `true` when `per_call_limit` is zero (limit disabled).
    pub fn within_per_call_limit(&self, amount: u64) -> bool {
        self.per_call_limit == 0 || amount <= self.per_call_limit
    }

    /// Returns `true` if adding `amount` to `current_spend` stays within the
    /// daily budget. Always returns `true` when `daily_budget` is zero (disabled).
    pub fn within_daily_budget(&self, current_spend: u64, amount: u64) -> bool {
        self.daily_budget == 0 || current_spend.saturating_add(amount) <= self.daily_budget
    }

    /// Returns `true` if adding `amount` to `current_spend` stays within the
    /// weekly budget. Always returns `true` when `weekly_budget` is zero (disabled).
    pub fn within_weekly_budget(&self, current_spend: u64, amount: u64) -> bool {
        self.weekly_budget == 0 || current_spend.saturating_add(amount) <= self.weekly_budget
    }

    /// Returns `true` if adding `amount` to `current_spend` stays within the
    /// monthly budget. Always returns `true` when `monthly_budget` is zero (disabled).
    pub fn within_monthly_budget(&self, current_spend: u64, amount: u64) -> bool {
        self.monthly_budget == 0 || current_spend.saturating_add(amount) <= self.monthly_budget
    }

    /// Returns `true` if the agent has not yet hit the per-window call cap.
    /// Always returns `true` when `rate_limit_calls` is zero (disabled).
    pub fn within_rate_limit(&self) -> bool {
        self.rate_limit_calls == 0 || self.rate_limit_call_count < self.rate_limit_calls
    }

    /// Resets the rate-limit call counter if the current window has elapsed.
    pub fn reset_rate_window_if_elapsed(&mut self, now: i64) {
        if self.rate_limit_window_secs > 0
            && now >= self.rate_limit_window_start + self.rate_limit_window_secs as i64
        {
            self.rate_limit_call_count = 0;
            self.rate_limit_window_start = now;
        }
    }
}
