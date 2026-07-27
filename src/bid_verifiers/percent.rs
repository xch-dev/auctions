use chia_wallet_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct PercentBidVerifierArgs {
    pub minimum: u64,
    pub increment_bps: u64,
}

impl PercentBidVerifierArgs {
    pub fn new(minimum: u64, increment_bps: u64) -> Self {
        Self {
            minimum,
            increment_bps,
        }
    }
}

compile_rue!(
    debug PercentBidVerifierArgs = PERCENT_BID_VERIFIER,
    "puzzles/bid_verifiers/percent_bid_verifier.rue"
);
