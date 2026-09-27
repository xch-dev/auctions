mod common;

use anyhow::Result;
use auction::*;
use chia_wallet_sdk::{prelude::*, puzzles::SETTLEMENT_PAYMENT_HASH, types::announcement_id};
use common::*;

const BIDS: [u64; 9] = [
    1,
    19,
    20,
    21,
    150,
    9_999,
    10_000,
    123_456_789,
    100_000_000_000_000,
];

fn assert_royalties(royalty_bps: u16, reserve: ReserveKind) -> Result<()> {
    for amount in BIDS {
        let mut h = Harness::new(Config {
            bid_verifier: BidVerifier::Flat {
                minimum_bid: 1,
                bid_increment: 1,
            },
            royalty_bps,
            reserve,
            ..Config::default()
        })?;

        let winner = h.bid(amount)?;
        assert_eq!(
            winner.reserve_amount,
            amount + fee(amount, 300) + fee(amount, royalty_bps.into())
        );

        h.set_time(END_TIME)?;
        let final_reserve = h.auction.info.reserve;
        let nft = h.end()?;

        h.assert_settled(final_reserve, amount);
        assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);
    }

    Ok(())
}

#[test]
fn xch_royalties() -> Result<()> {
    assert_royalties(500, ReserveKind::Xch)
}

#[test]
fn cat_royalties() -> Result<()> {
    assert_royalties(500, ReserveKind::Cat)
}

#[test]
fn revocable_cat_royalties() -> Result<()> {
    assert_royalties(500, ReserveKind::RevocableCat)
}

#[test]
fn small_royalties() -> Result<()> {
    assert_royalties(1, ReserveKind::Xch)?;
    assert_royalties(1, ReserveKind::Cat)
}

#[test]
fn full_royalty() -> Result<()> {
    assert_royalties(10_000, ReserveKind::Xch)?;
    assert_royalties(10_000, ReserveKind::Cat)
}

#[test]
fn no_royalty() -> Result<()> {
    assert_royalties(0, ReserveKind::Xch)?;
    assert_royalties(0, ReserveKind::Cat)
}

#[test]
fn refund_to_settlement_puzzle_hash_is_not_a_royalty() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let reserve_amount = h.auction.info.reserve_amount_for_bid(100).unwrap();
    let bidder = h.sim.bls(reserve_amount);
    let bid = Bid::new(100, SETTLEMENT_PAYMENT_HASH.into());
    let action = h.auction.spend_bid_action(&mut h.ctx, bid, false)?;
    StandardLayer::new(bidder.pk).spend(
        &mut h.ctx,
        bidder.coin,
        Conditions::new()
            .assert_coin_announcement(announcement_id(h.auction.coin.coin_id(), bid.tree_hash())),
    )?;
    let auction = h.auction.spend(&mut h.ctx, vec![action], vec![])?;
    h.sim.spend_coins(h.ctx.take(), &[bidder.sk])?;
    h.auction = auction;

    let reserve_before = h.auction.info.reserve;
    h.bid(200)?;

    // The driver must leave the refund alone instead of paying it to the royalty address
    h.assert_unspent(h.reserve_output(
        reserve_before,
        SETTLEMENT_PAYMENT_HASH.into(),
        reserve_amount,
    ));

    Ok(())
}
