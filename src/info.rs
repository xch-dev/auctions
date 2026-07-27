use chia_wallet_sdk::{
    prelude::*,
    puzzles::SETTLEMENT_PAYMENT_HASH,
    types::puzzles::{
        ActionLayerArgs, RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs,
    },
};

use crate::{
    AuctionReserve, AuctionSettings, AuctionState, BidActionArgs, BidVerifier, EndActionArgs,
    FlatBidVerifierArgs, NftUnlockerArgs, PercentBidVerifierArgs,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuctionInfo {
    pub launcher_id: Bytes32,
    pub settings: AuctionSettings,
    pub nft_coin_id: Bytes32,
    pub nft_royalty: RoyaltyInfo,
    pub state: AuctionState,
    pub reserve: AuctionReserve,
}

impl AuctionInfo {
    pub fn new(
        launcher_id: Bytes32,
        settings: AuctionSettings,
        nft_coin_id: Bytes32,
        nft_royalty: RoyaltyInfo,
        state: AuctionState,
        reserve: AuctionReserve,
    ) -> Self {
        Self {
            launcher_id,
            settings,
            nft_coin_id,
            nft_royalty,
            state,
            reserve,
        }
    }

    pub fn bid_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let bid_verifier = match self.settings.bid_verifier {
            BidVerifier::Flat {
                minimum_bid,
                bid_increment,
            } => ctx.curry(FlatBidVerifierArgs::new(minimum_bid, bid_increment))?,
            BidVerifier::Percent {
                minimum_bid,
                bid_increment_bps,
            } => ctx.curry(PercentBidVerifierArgs::new(minimum_bid, bid_increment_bps))?,
        };

        ctx.curry(BidActionArgs::new(
            bid_verifier,
            self.settings.timings,
            self.settings.payments.buyers_premium.bps + u64::from(self.nft_royalty.basis_points),
        ))
    }

    pub fn bid_action_hash(&self) -> Bytes32 {
        let bid_verifier_hash = match self.settings.bid_verifier {
            BidVerifier::Flat {
                minimum_bid,
                bid_increment,
            } => FlatBidVerifierArgs::new(minimum_bid, bid_increment).curry_tree_hash(),
            BidVerifier::Percent {
                minimum_bid,
                bid_increment_bps,
            } => PercentBidVerifierArgs::new(minimum_bid, bid_increment_bps).curry_tree_hash(),
        };

        BidActionArgs::new(
            bid_verifier_hash,
            self.settings.timings,
            self.settings.payments.buyers_premium.bps + u64::from(self.nft_royalty.basis_points),
        )
        .curry_tree_hash()
        .into()
    }

    pub fn end_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let has_royalty = self.nft_royalty.basis_points > 0;
        let unlocker = ctx.curry(NftUnlockerArgs::new(
            has_royalty.then(|| self.reserve.settlement_puzzle_hash()),
        ))?;

        ctx.curry(EndActionArgs::new(
            unlocker,
            self.settings.timings,
            self.settings.payments,
            self.nft_coin_id,
            has_royalty.then(|| SETTLEMENT_PAYMENT_HASH.into()),
        ))
    }

    pub fn end_action_hash(&self) -> Bytes32 {
        let has_royalty = self.nft_royalty.basis_points > 0;

        EndActionArgs::new(
            NftUnlockerArgs::new(has_royalty.then(|| self.reserve.settlement_puzzle_hash()))
                .curry_tree_hash(),
            self.settings.timings,
            self.settings.payments,
            self.nft_coin_id,
            has_royalty.then(|| SETTLEMENT_PAYMENT_HASH.into()),
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
