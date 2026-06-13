use anchor_lang::prelude::*;

/// Incremented whenever a breaking change is made to on-chain account layouts or
/// instruction encoding. Off-chain clients should assert this matches their
/// compiled-in expectation before submitting transactions.
pub const PROGRAM_VERSION: u8 = 2;

/// Singleton protocol-authority account. Initialised once by the program deployer.
///
/// Stores the `admin` pubkey authorised to freeze or unfreeze any AgentWallet
/// via the `freeze_agent` / `unfreeze_agent` instructions. The freeze authority
/// is deliberately separate from the per-wallet `owner` keypair so that the Maxim
/// Protocol security team can respond to a compromised agent without holding any
/// spending keys.
///
/// Freezing blocks new payment settlements but does NOT prevent the owner from
/// withdrawing their USDC — that keeps the protocol non-custodial even under
/// emergency conditions.
#[account]
pub struct ProtocolConfig {
    /// The Ed25519 public key authorised to freeze and unfreeze agent wallets.
    /// Rotated via `rotate_protocol_admin`.
    pub admin: Pubkey,

    /// On-chain mirror of `PROGRAM_VERSION`. Off-chain clients can read this
    /// to confirm they are speaking to the expected program revision before
    /// submitting transactions.
    pub version: u8,

    /// Protocol-wide cap on the USDC amount of any single payment settlement.
    /// 6-decimal fixed-point. Zero disables the cap (no protocol-level limit).
    /// This is a hard ceiling enforced across all agent wallets regardless of
    /// their individual spend policies. The admin can lower this during an
    /// incident to limit further exposure without freezing individual agents.
    pub max_single_payment_usdc: u64,

    /// PDA canonical bump seed.
    pub bump: u8,
}

impl ProtocolConfig {
    pub const LEN: usize = 8  // Anchor discriminator
        + 32                   // admin
        + 1                    // version
        + 8                    // max_single_payment_usdc
        + 1;                   // bump
}
