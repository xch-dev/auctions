use chia_wallet_sdk::prelude::*;

use crate::Bid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NftUnlocker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NftUnlockerSolution {
    pub winning_bid: Bid,
    pub nft_amount: u64,
}

impl NftUnlockerSolution {
    pub fn new(winning_bid: Bid, nft_amount: u64) -> Self {
        Self {
            winning_bid,
            nft_amount,
        }
    }
}

compile_rue!(
    debug NftUnlocker = NFT_UNLOCKER,
    "puzzles/unlockers/nft_unlocker.rue"
);
