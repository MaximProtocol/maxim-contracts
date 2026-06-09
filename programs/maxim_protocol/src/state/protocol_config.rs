use anchor_lang::prelude::*;

/// Incremented whenever a breaking change is made to on-chain account layouts or
/// instruction encoding. Off-chain clients should assert this matches their
/// compiled-in expectation before submitting transactions.
pub const PROGRAM_VERSION: u8 = 1;

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

    /// PDA canonical bump seed.
    pub bump: u8,
}

impl ProtocolConfig {
    pub const LEN: usize = 8  // Anchor discriminator
        + 32                   // admin
        + 1;                   // bump
}
