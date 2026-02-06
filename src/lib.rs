mod actions;
mod auction;
mod bid_verifiers;
mod info;
mod launcher;
mod p2;
mod types;

pub use actions::*;
pub use auction::*;
pub use bid_verifiers::*;
pub use info::*;
pub use launcher::*;
pub use p2::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use std::slice;

    use anyhow::Result;
    use chia_wallet_sdk::prelude::*;

    use crate::{
        AuctionAsset, AuctionExt, AuctionLauncherExt, AuctionSettings, BpsPayment, Payments,
        Timings, auction_lock_p2_puzzle_hash, spend_auction_lock,
    };

    #[test]
    fn test_auction() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1002);
        let alice_p2 = StandardLayer::new(alice.pk);

        let bp = sim.bls(0);
        let commission = sim.bls(0);

        let (mint_nft, nft) = Launcher::new(alice.coin.coin_id(), 0)
            .with_singleton_amount(1)
            .mint_nft(
                &mut ctx,
                &NftMint::new(HashedPtr::NIL, alice.puzzle_hash, 0, None),
            )?;

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let p2_puzzle_hash = auction_lock_p2_puzzle_hash(launcher.coin().coin_id());
        let hint = ctx.hint(p2_puzzle_hash)?;

        let locked_nft = nft.spend_with(
            &mut ctx,
            &alice_p2,
            Conditions::new().create_coin(p2_puzzle_hash, 1, Memos::None),
        )?;

        let (launch_auction, auction) = launcher.launch_auction(
            &mut ctx,
            AuctionSettings {
                minimum_bid: 1,
                bid_increment: 100,
                timings: Timings::new(5, 0),
                payments: Payments {
                    buyers_premium: BpsPayment::new(300, bp.puzzle_hash),
                    commission: BpsPayment::new(100, commission.puzzle_hash),
                    payout_puzzle_hash: alice.puzzle_hash,
                },
            },
            AuctionAsset::Xch,
            locked_nft.coin.coin_id(),
        )?;

        let reserve_coin = Coin::new(alice.coin.coin_id(), p2_puzzle_hash, 0);

        alice_p2.spend(
            &mut ctx,
            alice.coin,
            mint_nft
                .extend(launch_auction)
                .create_coin(reserve_coin.puzzle_hash, 0, Memos::None)
                .create_coin(alice.puzzle_hash, 1000, Memos::None),
        )?;

        let remainder = Coin::new(alice.coin.coin_id(), alice.puzzle_hash, 1000);

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let auction = auction.spend(&mut ctx, reserve_coin.parent_coin_info, vec![])?;

        let reserve_spend = spend_auction_lock(
            &mut ctx,
            auction.info.launcher_id,
            auction.info.inner_puzzle_hash(),
            Conditions::new().create_coin(p2_puzzle_hash, 0, hint),
        )?;
        ctx.spend(reserve_coin, reserve_spend)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        Ok(())
    }
}
