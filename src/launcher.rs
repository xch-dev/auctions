use chia_wallet_sdk::{
    chia::puzzle_types::{EveProof, Proof},
    prelude::*,
};

use crate::{Auction, AuctionAsset, AuctionInfo, AuctionMemo, AuctionSettings, AuctionState};

pub trait AuctionLauncherExt {
    fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        asset: AuctionAsset,
        nft_coin_id: Bytes32,
    ) -> Result<(Conditions, Auction), DriverError>;
}

impl AuctionLauncherExt for Launcher {
    fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        asset: AuctionAsset,
        nft_coin_id: Bytes32,
    ) -> Result<(Conditions, Auction), DriverError> {
        let launcher_coin = self.coin();

        let info = AuctionInfo::new(
            launcher_coin.coin_id(),
            settings,
            asset,
            nft_coin_id,
            AuctionState::initial(settings.payments.payout_puzzle_hash),
        );

        let (conditions, coin) = self.spend(
            ctx,
            info.inner_puzzle_hash().into(),
            AuctionMemo { settings },
        )?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((conditions, Auction::new(coin, proof, info)))
    }
}
