use anchor_lang::prelude::*;

#[error_code]
pub enum MaximError {
    #[msg("Agent ID must be between 1 and 64 characters")]
    AgentIdTooLong,

    #[msg("Agent wallet is not active")]
    AgentNotActive,

    #[msg("Agent wallet is frozen by the protocol admin")]
    AgentFrozen,

    #[msg("Unauthorized: signer is not the registered agent owner")]
    Unauthorized,

    #[msg("Payment amount must be greater than zero")]
    ZeroPaymentAmount,

    #[msg("Invalid protocol discriminant -- expected 0 (x402) or 1 (MPP)")]
    InvalidProtocol,

    #[msg("Daily budget cap exceeded")]
    DailyBudgetExceeded,

    #[msg("Weekly budget cap exceeded")]
    WeeklyBudgetExceeded,

    #[msg("Per-call payment limit exceeded")]
    PerCallLimitExceeded,

    #[msg("Rate limit window exceeded")]
    RateLimitExceeded,

    #[msg("Destination domain is not in the allowlist")]
    DomainNotAllowed,

    #[msg("Destination domain is in the blocklist")]
    DomainBlocked,

    #[msg("Domain list exceeds the maximum of 32 entries")]
    DomainListFull,

    #[msg("Parent payment record does not belong to the same agent wallet")]
    ParentAgentMismatch,

    #[msg("New owner pubkey must not be the default (zero) address")]
    InvalidOwner,

    #[msg("Withdrawal amount exceeds available token account balance")]
    InsufficientFunds,

    #[msg("ProtocolConfig has already been initialised")]
    ProtocolAlreadyInitialised,
}
