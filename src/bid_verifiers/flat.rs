use chia_wallet_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct FlatBidVerifierArgs {
    pub minimum: u64,
    pub increment: u64,
}

impl FlatBidVerifierArgs {
    pub fn new(minimum: u64, increment: u64) -> Self {
        Self { minimum, increment }
    }
}

compile_rue!(
    debug FlatBidVerifierArgs = FLAT_BID_VERIFIER,
    "puzzles/bid_verifiers/flat_bid_verifier.rue"
);
