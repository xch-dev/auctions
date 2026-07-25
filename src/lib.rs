mod actions;
mod auction;
mod auction_reserve;
mod bid_verifiers;
mod info;
mod launcher;
mod p2;
mod types;
mod unlockers;

pub use actions::*;
pub use auction::*;
pub use auction_reserve::*;
pub use bid_verifiers::*;
pub use info::*;
pub use launcher::*;
pub use p2::*;
pub use types::*;
pub use unlockers::*;

#[cfg(test)]
mod tests {
    use std::slice;

    use anyhow::Result;
    use chia_wallet_sdk::{
        driver::calculate_nft_royalty, prelude::*, puzzles::SETTLEMENT_PAYMENT_HASH,
    };

    use crate::{
        AuctionExt, AuctionLauncherExt, AuctionReserve, AuctionSettings, Bid, BpsPayment, Payments,
        Timings, auction_lock_p2_puzzle_hash,
    };

    #[test]
    fn test_auction() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1002);
        let alice_p2 = StandardLayer::new(alice.pk);

        let bob = sim.bls(1000);
        let bob_p2 = StandardLayer::new(bob.pk);

        let (mint_nft, nft) = Launcher::new(alice.coin.coin_id(), 0)
            .with_singleton_amount(1)
            .mint_nft(
                &mut ctx,
                &NftMint::new(HashedPtr::NIL, alice.puzzle_hash, 500, None),
            )?;

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let p2_puzzle_hash = auction_lock_p2_puzzle_hash(launcher.coin().coin_id());

        let locked_nft = nft.spend_with(
            &mut ctx,
            &alice_p2,
            Conditions::new().create_coin(p2_puzzle_hash, 1, Memos::None),
        )?;

        let reserve_coin = Coin::new(alice.coin.coin_id(), p2_puzzle_hash, 0);

        let (launch_auction, auction) = launcher.launch_auction(
            &mut ctx,
            AuctionSettings {
                minimum_bid: 100,
                bid_increment: 100,
                timings: Timings::new(5, 0),
                payments: Payments {
                    buyers_premium: BpsPayment::new(300, Bytes32::new([1; 32])),
                    commission: BpsPayment::new(100, Bytes32::new([2; 32])),
                    payout_puzzle_hash: alice.puzzle_hash,
                },
            },
            AuctionReserve::Xch(reserve_coin),
            &locked_nft,
        )?;

        alice_p2.spend(
            &mut ctx,
            alice.coin,
            mint_nft
                .extend(launch_auction)
                .create_coin(reserve_coin.puzzle_hash, 0, Memos::None)
                .create_coin(alice.puzzle_hash, 1000, Memos::None),
        )?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let bid_action =
            auction.spend_bid_action(&mut ctx, Bid::new(100, bob.puzzle_hash), false)?;
        let auction = auction.spend(&mut ctx, vec![bid_action], vec![])?;
        bob_p2.spend(&mut ctx, bob.coin, Conditions::new())?;

        sim.spend_coins(ctx.take(), slice::from_ref(&bob.sk))?;

        sim.set_next_timestamp(5)?;

        let reserve_coin_id = auction.info.reserve.coin().coin_id();
        let end_action = auction.spend_end_action(&mut ctx, &locked_nft)?;
        let _nft = auction.unlock_nft(&mut ctx, &locked_nft)?;
        let _auction = auction.spend(&mut ctx, vec![end_action], vec![])?;

        sim.spend_coins(ctx.take(), &[])?;

        let settlement_coin = Coin::new(reserve_coin_id, SETTLEMENT_PAYMENT_HASH.into(), 0);
        let royalty_coin = Coin::new(
            settlement_coin.coin_id(),
            alice.puzzle_hash,
            calculate_nft_royalty(100, 500),
        );
        assert!(sim.coin_state(royalty_coin.coin_id()).is_some());

        Ok(())
    }
}
