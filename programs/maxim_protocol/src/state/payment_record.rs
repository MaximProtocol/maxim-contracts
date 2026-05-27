use anchor_lang::prelude::*;

/// Identifies which machine payment protocol executed the upstream handshake.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaymentProtocol {
    /// x402 -- HTTP 402-native, USDC-settled on Solana (Coinbase standard).
    /// The payment handshake is embedded in the HTTP request/response cycle.
    /// Settlement is the SPL Token transfer executed by the `settle_payment` instruction.
    X402 = 0,

    /// MPP -- Machine Payments Protocol IETF draft (Stripe + Tempo).
    /// The MPP handshake and off-chain rail settlement occur before this instruction.
    /// The on-chain record mirrors the settlement for transparency and auditability.
    Mpp = 1,
}

impl PaymentProtocol {
    pub fn from_discriminant(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::X402),
            1 => Some(Self::Mpp),
            _ => None,
        }
    }
}

/// Immutable on-chain record of a single agent payment or policy violation.
///
/// One `PaymentRecord` account is created per payment event -- whether that event
/// is a successful settlement or a rejected policy violation. Accounts are
/// PDA-derived from `[b"payment_record", agent_wallet, sequence]` and are never
/// mutated after creation, making the on-chain ledger append-only and tamper-evident.
///
/// The full endpoint URL is stored encrypted in the Maxim Protocol off-chain database
/// and visible in the dashboard. Only the SHA-256 hash appears on-chain, so sensitive
/// service endpoints are not publicly exposed on Solana Explorer.
///
/// Payment chains -- where an orchestrating agent delegates to sub-agents -- are
/// traceable on-chain via the `parent_payment` field. Starting from any root
/// `PaymentRecord`, the complete cost of a multi-agent workflow can be reconstructed
/// by following the `parent_payment` links.
#[account]
pub struct PaymentRecord {
    /// The AgentWallet PDA that originated this payment.
    pub agent_wallet: Pubkey,

    /// Monotonic sequence number within this agent's payment history.
    /// Matches the `payment_sequence` value on the AgentWallet at settlement time.
    /// Used as a PDA seed to ensure uniqueness per agent.
    pub sequence: u64,

    /// SHA-256 hash of the full destination service endpoint URL.
    /// The gateway indexes the full URL in its off-chain database.
    pub endpoint_hash: [u8; 32],

    /// The receiving counterparty's Solana public key.
    /// For successful payments, this is the payee's USDC token account owner.
    /// Set to `Pubkey::default()` for policy violation records.
    pub payee: Pubkey,

    /// USDC amount involved in this event (6-decimal fixed-point).
    /// For violations, this is the requested amount that was rejected.
    pub amount_usdc: u64,

    /// The protocol that executed the upstream payment handshake.
    pub protocol: PaymentProtocol,

    /// Unix timestamp of on-chain settlement confirmation.
    pub settled_at: i64,

    /// `true` if the payment passed all spend policy checks and funds were
    /// transferred. `false` if this record documents a policy violation (no
    /// funds moved; the record exists for auditability only).
    pub policy_passed: bool,

    /// Optional reference to a parent `PaymentRecord` PDA.
    ///
    /// Set when this payment is part of a multi-agent orchestration chain --
    /// i.e., a sub-agent payment funded by an orchestrating agent's budget.
    /// `None` for top-level payments originating directly from user or
    /// orchestrator code.
    pub parent_payment: Option<Pubkey>,

    /// PDA canonical bump seed.
    pub bump: u8,
}

impl PaymentRecord {
    pub const LEN: usize = 8       // Anchor discriminator
        + 32                        // agent_wallet
        + 8                         // sequence
        + 32                        // endpoint_hash
        + 32                        // payee
        + 8                         // amount_usdc
        + 1                         // protocol (enum stored as u8)
        + 8                         // settled_at
        + 1                         // policy_passed
        + 1 + 32                    // parent_payment: Option<Pubkey> (1-byte discriminant + Pubkey)
        + 1;                        // bump
}
