use chia_wallet_sdk::prelude::*;

use crate::Bid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct NftUnlockerArgs {
    pub settlement_puzzle_hash: Option<Bytes32>,
}

impl NftUnlockerArgs {
    pub fn new(settlement_puzzle_hash: Option<Bytes32>) -> Self {
        Self {
            settlement_puzzle_hash,
        }
    }
}

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
    debug NftUnlockerArgs = NFT_UNLOCKER,
    "puzzles/unlockers/nft_unlocker.rue"
);
