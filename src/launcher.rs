use chia_wallet_sdk::{
    chia::puzzle_types::{EveProof, Proof},
    prelude::*,
};

use crate::{
    AUCTION_SINGLETON_AMOUNT, Auction, AuctionError, AuctionInfo, AuctionReserve, AuctionSettings,
    AuctionState, validate_auction,
};

pub trait AuctionLauncherExt {
    /// Launches an auction for `nft`, which must already be locked by the auction (its p2 puzzle hash
    /// must be [`auction_lock_p2_puzzle_hash`](crate::auction_lock_p2_puzzle_hash) of the launcher).
    fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        reserve: AuctionReserve,
        nft: &Nft,
    ) -> Result<(Conditions, Auction), AuctionError>;
}

impl AuctionLauncherExt for Launcher {
    fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        reserve: AuctionReserve,
        nft: &Nft,
    ) -> Result<(Conditions, Auction), AuctionError> {
        let launcher_coin = self.coin();

        let info = AuctionInfo::new(
            launcher_coin.coin_id(),
            settings,
            nft.coin.coin_id(),
            RoyaltyInfo::new(
                nft.info.launcher_id,
                nft.info.royalty_puzzle_hash,
                nft.info.royalty_basis_points,
            ),
            AuctionState::initial(settings.payments.payout_puzzle_hash),
            reserve,
        );

        validate_auction(&info)?;

        if self.singleton_amount() != AUCTION_SINGLETON_AMOUNT {
            return Err(AuctionError::InvalidSingletonAmount(
                self.singleton_amount(),
            ));
        }

        let (conditions, coin) = self.spend(ctx, info.inner_puzzle_hash().into(), info.memo())?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((conditions, Auction::new(coin, proof, info)))
    }
}
