use chia_wallet_sdk::{
    prelude::*,
    types::puzzles::{
        ActionLayerArgs, RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs,
    },
};

use crate::{
    AuctionAsset, AuctionSettings, AuctionState, BidActionArgs, EndActionArgs, FlatBidVerifierArgs,
    auction_lock_p2_puzzle_hash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuctionInfo {
    pub launcher_id: Bytes32,
    pub settings: AuctionSettings,
    pub asset: AuctionAsset,
    pub nft_coin_id: Bytes32,
    pub state: AuctionState,
}

impl AuctionInfo {
    pub fn new(
        launcher_id: Bytes32,
        settings: AuctionSettings,
        asset: AuctionAsset,
        nft_coin_id: Bytes32,
        state: AuctionState,
    ) -> Self {
        Self {
            launcher_id,
            settings,
            asset,
            nft_coin_id,
            state,
        }
    }

    pub fn bid_action(&self) -> BidActionArgs<FlatBidVerifierArgs> {
        BidActionArgs::new(
            FlatBidVerifierArgs::new(self.settings.minimum_bid, self.settings.bid_increment),
            self.settings.timings,
            self.settings.payments.buyers_premium.bps,
        )
    }

    pub fn end_action(&self) -> EndActionArgs {
        EndActionArgs::new(
            self.settings.timings,
            self.settings.payments,
            self.nft_coin_id,
        )
    }

    pub fn merkle_leaves(&self) -> [Bytes32; 2] {
        [
            self.bid_action().curry_tree_hash().into(),
            self.end_action().curry_tree_hash().into(),
        ]
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&self.merkle_leaves())
    }

    pub fn locked_p2_puzzle_hash(&self) -> Bytes32 {
        auction_lock_p2_puzzle_hash(self.launcher_id)
    }

    pub fn locked_full_puzzle_hash(&self) -> Bytes32 {
        let p2_puzzle_hash = self.locked_p2_puzzle_hash();

        match self.asset {
            AuctionAsset::Xch => p2_puzzle_hash,
            AuctionAsset::Cat { asset_id } => CatInfo::new(asset_id, None, p2_puzzle_hash)
                .puzzle_hash()
                .into(),
            AuctionAsset::RevocableCat {
                asset_id,
                hidden_puzzle_hash,
            } => CatInfo::new(asset_id, Some(hidden_puzzle_hash), p2_puzzle_hash)
                .puzzle_hash()
                .into(),
        }
    }
}

impl SingletonInfo for AuctionInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            ReserveFinalizer2ndCurryArgs::curry_tree_hash(
                self.locked_full_puzzle_hash(),
                self.locked_p2_puzzle_hash(),
                RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
                self.launcher_id,
            ),
            self.merkle_tree().root(),
            self.state.tree_hash(),
        )
    }
}
