use chia_wallet_sdk::{prelude::*, types::puzzles::ActionLayerArgs};

use crate::BidActionArgs;

type Auction = Singleton<AuctionInfo>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuctionInfo {
    pub launcher_id: Bytes32,
}

impl AuctionInfo {
    fn bid_action() -> BidActionArgs {
        todo!()
    }
}

impl SingletonInfo for AuctionInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        // ActionLayerArgs::curry_tree_hash(finalizer, state_hash)
        todo!()
    }
}
