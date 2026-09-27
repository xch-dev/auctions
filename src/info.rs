use chia_wallet_sdk::{
    prelude::*,
    puzzles::SETTLEMENT_PAYMENT_HASH,
    types::puzzles::{
        ActionLayerArgs, RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs,
    },
};

use crate::{
    AuctionMemo, AuctionReserve, AuctionSettings, AuctionState, BidActionArgs, BidVerifier,
    EndActionArgs, FlatBidVerifierArgs, NftUnlockerArgs, PercentBidVerifierArgs, ReserveMemo,
    auction_lock_p2_puzzle_hash, calculate_bps_payment,
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

    /// Reconstructs a newly launched auction from its launcher memo, and the royalty info of the NFT
    /// at the memo's NFT coin ID.
    pub fn from_memo(launcher_id: Bytes32, memo: AuctionMemo, nft_royalty: RoyaltyInfo) -> Self {
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(launcher_id);

        let reserve = match memo.reserve {
            ReserveMemo::Xch { parent_coin_info } => {
                AuctionReserve::Xch(Coin::new(parent_coin_info, lock_puzzle_hash, 0))
            }
            ReserveMemo::Cat {
                parent_coin_info,
                lineage_proof,
                asset_id,
                hidden_puzzle_hash,
            } => {
                let info = CatInfo::new(asset_id, hidden_puzzle_hash, lock_puzzle_hash);
                let coin = Coin::new(parent_coin_info, info.puzzle_hash().into(), 0);
                AuctionReserve::Cat(Cat::new(coin, lineage_proof, info))
            }
        };

        Self::new(
            launcher_id,
            memo.settings,
            memo.nft_coin_id,
            nft_royalty,
            AuctionState::initial(memo.settings.payments.payout_puzzle_hash),
            reserve,
        )
    }

    /// The launcher memo for this auction, which is only meaningful before its first spend.
    pub fn memo(&self) -> AuctionMemo {
        let reserve = match self.reserve {
            AuctionReserve::Xch(coin) => ReserveMemo::Xch {
                parent_coin_info: coin.parent_coin_info,
            },
            AuctionReserve::Cat(cat) => ReserveMemo::Cat {
                parent_coin_info: cat.coin.parent_coin_info,
                lineage_proof: cat.lineage_proof,
                asset_id: cat.info.asset_id,
                hidden_puzzle_hash: cat.info.hidden_puzzle_hash,
            },
        };

        AuctionMemo {
            settings: self.settings,
            nft_coin_id: self.nft_coin_id,
            reserve,
        }
    }

    pub fn royalty_bps(&self) -> u64 {
        self.nft_royalty.basis_points.into()
    }

    /// The total amount a bidder must lock up for a bid of `bid_amount`, which is the bid plus the
    /// buyer's premium and NFT royalty. It's refunded in full if the bid is outbid.
    pub fn reserve_amount_for_bid(&self, bid_amount: u64) -> Option<u64> {
        let premium = calculate_bps_payment(bid_amount, self.settings.payments.buyers_premium.bps)?;
        let royalty = calculate_bps_payment(bid_amount, self.royalty_bps())?;
        bid_amount.checked_add(premium)?.checked_add(royalty)
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
            self.settings.payments.buyers_premium.bps,
            self.royalty_bps(),
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
            self.settings.payments.buyers_premium.bps,
            self.royalty_bps(),
        )
        .curry_tree_hash()
        .into()
    }

    fn nft_unlocker_args(&self) -> NftUnlockerArgs {
        NftUnlockerArgs::new(self.reserve.settlement_puzzle_hash(), self.royalty_bps())
    }

    pub fn nft_unlocker(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(self.nft_unlocker_args())
    }

    pub fn end_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let unlocker = self.nft_unlocker(ctx)?;

        ctx.curry(EndActionArgs::new(
            unlocker,
            self.settings.timings,
            self.settings.payments,
            self.royalty_bps(),
            SETTLEMENT_PAYMENT_HASH.into(),
            self.nft_coin_id,
        ))
    }

    pub fn end_action_hash(&self) -> Bytes32 {
        EndActionArgs::new(
            self.nft_unlocker_args().curry_tree_hash(),
            self.settings.timings,
            self.settings.payments,
            self.royalty_bps(),
            SETTLEMENT_PAYMENT_HASH.into(),
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
