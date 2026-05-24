import * as anchor from "@coral-xyz/anchor";

// No on-chain migration steps are required for the initial deployment.
// The program is deployed via `anchor deploy` or `anchor build && solana program deploy`.
// Agent wallets and spend policies are initialised via the SDK at runtime.
module.exports = async function (_provider: anchor.AnchorProvider) {
  anchor.setProvider(_provider);
};
