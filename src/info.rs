use chia_wallet_sdk::{
    prelude::*,
    types::puzzles::{
        ActionLayerArgs, RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs,
    },
};

use crate::{
    AuctionReserve, AuctionSettings, AuctionState, BidActionArgs, EndActionArgs,
    FlatBidVerifierArgs,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuctionInfo {
    pub launcher_id: Bytes32,
    pub settings: AuctionSettings,
    pub nft_coin_id: Bytes32,
    pub state: AuctionState,
    pub reserve: AuctionReserve,
}

impl AuctionInfo {
    pub fn new(
        launcher_id: Bytes32,
        settings: AuctionSettings,
        nft_coin_id: Bytes32,
        state: AuctionState,
        reserve: AuctionReserve,
    ) -> Self {
        Self {
            launcher_id,
            settings,
            nft_coin_id,
            state,
            reserve,
        }
    }

    pub fn bid_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let bid_verifier = ctx.curry(FlatBidVerifierArgs::new(
            self.settings.minimum_bid,
            self.settings.bid_increment,
        ))?;

        ctx.curry(BidActionArgs::new(
            bid_verifier,
            self.settings.timings,
            self.settings.payments.buyers_premium.bps,
        ))
    }

    pub fn bid_action_hash(&self) -> Bytes32 {
        BidActionArgs::new(
            FlatBidVerifierArgs::new(self.settings.minimum_bid, self.settings.bid_increment)
                .curry_tree_hash(),
            self.settings.timings,
            self.settings.payments.buyers_premium.bps,
        )
        .curry_tree_hash()
        .into()
    }

    pub fn end_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(EndActionArgs::new(
            self.settings.timings,
            self.settings.payments,
            self.nft_coin_id,
        ))
    }

    pub fn end_action_hash(&self) -> Bytes32 {
        EndActionArgs::new(
            self.settings.timings,
            self.settings.payments,
            self.nft_coin_id,
        )
        .curry_tree_hash()
        .into()
    }

    pub fn merkle_leaves(&self) -> [Bytes32; 2] {
        [self.bid_action_hash(), self.end_action_hash()]
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&self.merkle_leaves())
    }
}

impl SingletonInfo for AuctionInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            ReserveFinalizer2ndCurryArgs::curry_tree_hash(
                self.reserve.coin().puzzle_hash,
                self.reserve.p2_puzzle_hash(),
                RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
                self.launcher_id,
            ),
            self.merkle_tree().root(),
            self.state.tree_hash(),
        )
    }
}
