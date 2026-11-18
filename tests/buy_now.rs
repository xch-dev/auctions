mod common;

use anyhow::{Result, anyhow};
use auction::*;
use chia_wallet_sdk::prelude::*;
use common::*;

const BUY_NOW_PRICE: u64 = 1_000;

const ALL_RESERVES: [ReserveKind; 3] = [
    ReserveKind::Xch,
    ReserveKind::Cat,
    ReserveKind::RevocableCat,
];

/// The error for a spend bundle where the auction's puzzle raises during consensus validation.
const PUZZLE_RAISED: &str = "GeneratorRuntimeError";

fn with_buy_now(buy_now_price: u64) -> Config {
    Config {
        buy_now_price: Some(buy_now_price),
        ..Config::default()
    }
}

/// Places a bid without running it in the driver first, so that it's only checked on chain. The
/// bidder funds the reserve amount for the bid.
fn unchecked_bid(h: &mut Harness, amount: u64) -> Result<()> {
    let reserve_amount = h
        .auction
        .info
        .reserve_amount_for_bid(amount)
        .ok_or(anyhow!("reserve amount overflow"))?;
    let key = h.sim.bls(reserve_amount);
    StandardLayer::new(key.pk).spend(&mut h.ctx, key.coin, Conditions::new())?;
    let bid = h
        .auction
        .spend_bid_action(&mut h.ctx, Bid::new(amount, key.puzzle_hash), false)?;
    h.spend_unchecked(vec![bid], reserve_amount, &[key.sk])
}

/// Asserts that the driver refuses to buy the auction now.
fn assert_buy_now_unavailable(h: &mut Harness) {
    let nft = h.nft;
    let result = h
        .auction
        .spend_buy_now_actions(&mut h.ctx, h.seller.puzzle_hash, false, &nft);
    assert!(
        matches!(result, Err(AuctionError::BuyNowUnavailable)),
        "{result:?}"
    );
    h.ctx.take();
}

#[test]
fn buy_now_ends_the_auction_right_away() -> Result<()> {
    for reserve in ALL_RESERVES {
        let mut h = Harness::new(Config {
            reserve,
            ..with_buy_now(BUY_NOW_PRICE)
        })?;

        assert_eq!(
            h.auction.info.available_buy_now_price(),
            Some(BUY_NOW_PRICE)
        );

        // Well before the end time
        h.set_time(10)?;
        let reserve_before = h.auction.info.reserve;
        let (buyer, nft) = h.buy_now(false)?;

        assert_eq!(buyer.bid.amount, BUY_NOW_PRICE);
        assert_eq!(
            buyer.reserve_amount,
            BUY_NOW_PRICE + fee(BUY_NOW_PRICE, 300) + fee(BUY_NOW_PRICE, 500)
        );

        h.assert_unspent(nft.coin);
        assert_eq!(nft.info.p2_puzzle_hash, buyer.bid.puzzle_hash);
        assert_eq!(nft.info.current_owner, None);

        assert!(h.auction.info.state.ended);
        assert_eq!(h.auction.info.state.winning_bid, buyer.bid);
        assert_eq!(h.auction.info.available_buy_now_price(), None);

        // The bid and end are in the same spend, so the reserve pays out the buyer's funds directly
        h.assert_paid_out(reserve_before, BUY_NOW_PRICE);

        // A bought auction can't be bid on, ended, cancelled, or bought again
        assert_fails_with(h.bid(BUY_NOW_PRICE), "raise");
        assert_fails_with(h.end(), "raise");
        assert_fails_with(h.cancel(), "raise");
        assert_buy_now_unavailable(&mut h);
    }

    Ok(())
}

#[test]
fn buy_now_refunds_the_winning_bid() -> Result<()> {
    for reserve in ALL_RESERVES {
        let mut h = Harness::new(Config {
            reserve,
            ..with_buy_now(BUY_NOW_PRICE)
        })?;

        let bidder = h.bid(500)?;
        assert_eq!(
            h.auction.info.available_buy_now_price(),
            Some(BUY_NOW_PRICE)
        );

        let outbid_reserve = h.auction.info.reserve;
        let (buyer, nft) = h.buy_now(false)?;

        h.assert_unspent(h.reserve_output(
            outbid_reserve,
            bidder.bid.puzzle_hash,
            bidder.reserve_amount,
        ));
        assert_eq!(nft.info.p2_puzzle_hash, buyer.bid.puzzle_hash);
        h.assert_paid_out(outbid_reserve, BUY_NOW_PRICE);
    }

    Ok(())
}

#[test]
fn buy_now_after_a_pending_bid_in_the_same_spend() -> Result<()> {
    for reserve in ALL_RESERVES {
        let mut h = Harness::new(Config {
            reserve,
            ..with_buy_now(BUY_NOW_PRICE)
        })?;

        let reserve_before = h.auction.info.reserve;
        let (bidders, nft) = h.bids_then_buy_now(&[(100, false), (200, false)], false)?;
        let buyer = bidders[2];

        assert_eq!(nft.info.p2_puzzle_hash, buyer.bid.puzzle_hash);
        assert!(h.auction.info.state.ended);
        assert_eq!(h.auction.info.state.winning_bid, buyer.bid);

        // Every action in the spend is paid out by the same reserve coin, including the refunds of
        // the bids that were placed before the buy now
        for bidder in &bidders[..2] {
            h.assert_unspent(h.reserve_output(
                reserve_before,
                bidder.bid.puzzle_hash,
                bidder.reserve_amount,
            ));
        }
        h.assert_paid_out(reserve_before, BUY_NOW_PRICE);
    }

    Ok(())
}

#[test]
fn bid_at_buy_now_price_skips_the_bid_verifier() -> Result<()> {
    // With either verifier, the next bid after 1,000 must be at least 1,100, so the buy now price of
    // 1,050 is only reachable because it skips the verifier
    for bid_verifier in [
        BidVerifier::Flat {
            minimum_bid: 100,
            bid_increment: 100,
        },
        BidVerifier::Percent {
            minimum_bid: 100,
            bid_increment_bps: 1_000,
        },
    ] {
        let mut h = Harness::new(Config {
            bid_verifier,
            ..with_buy_now(1_050)
        })?;

        h.bid(1_000)?;
        assert_fails_with(h.bid(1_049), "raise");

        let (buyer, _) = h.buy_now(false)?;
        assert_eq!(buyer.bid.amount, 1_050);
    }

    Ok(())
}

#[test]
fn buy_now_price_can_equal_the_minimum_bid() -> Result<()> {
    let mut h = Harness::new(with_buy_now(100))?;

    // The only bid that can be placed is the buy now
    assert_fails_with(h.bid(101), "raise");
    let (buyer, _) = h.buy_now(false)?;
    assert_eq!(buyer.bid.amount, 100);

    Ok(())
}

#[test]
fn bids_cannot_exceed_buy_now_price() -> Result<()> {
    let mut h = Harness::new(with_buy_now(BUY_NOW_PRICE))?;

    assert_fails_with(h.bid(BUY_NOW_PRICE + 1), "raise");
    assert_fails_with(h.bid(BUY_NOW_PRICE * 10), "raise");

    // The checks are also enforced on chain, not just by the driver
    let result = unchecked_bid(&mut h, BUY_NOW_PRICE + 1);
    assert_fails_with(result, PUZZLE_RAISED);
    unchecked_bid(&mut h, BUY_NOW_PRICE)?;

    // A bid at the buy now price can never be outbid
    assert_fails_with(h.bid(BUY_NOW_PRICE), "raise");
    assert_fails_with(h.bid(BUY_NOW_PRICE + 100), "raise");

    Ok(())
}

#[test]
fn bid_at_buy_now_price_can_be_ended_by_anyone() -> Result<()> {
    // A wallet should always buy with the bid and end in the same spend, but if a bid at the buy now
    // price is placed on its own, the auction can still be ended right away
    let mut h = Harness::new(with_buy_now(BUY_NOW_PRICE))?;

    h.set_time(10)?;
    let winner = h.bid(BUY_NOW_PRICE)?;
    assert_eq!(h.auction.info.available_buy_now_price(), None);
    assert_buy_now_unavailable(&mut h);

    let settled_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);
    h.assert_settled(settled_reserve, BUY_NOW_PRICE);

    Ok(())
}

#[test]
fn end_waits_for_the_end_time_below_the_buy_now_price() -> Result<()> {
    for bid in [None, Some(BUY_NOW_PRICE - 1)] {
        let mut h = Harness::new(Config {
            timings: Timings::new(END_TIME, 100),
            ..with_buy_now(BUY_NOW_PRICE)
        })?;

        if let Some(amount) = bid {
            h.set_time(END_TIME - 50)?;
            h.bid(amount)?;
        }

        h.set_time(END_TIME - 1)?;
        assert_fails_with(h.end(), "AssertSecondsAbsoluteFailed");

        // The grace period still applies after a bid below the buy now price
        if bid.is_some() {
            h.set_time(END_TIME)?;
            assert_fails_with(h.end(), "AssertSecondsRelativeFailed");
        }

        h.set_time(END_TIME + 100)?;
        let _ = h.end()?;
    }

    Ok(())
}

#[test]
fn end_waits_for_the_end_time_without_a_buy_now_price() -> Result<()> {
    // With no buy now price, the price is curried in as 0, which must not match an auction with no
    // bids
    let mut h = Harness::new(Config::default())?;
    assert_eq!(h.auction.info.available_buy_now_price(), None);

    h.set_time(END_TIME - 1)?;
    assert_fails_with(h.end(), "AssertSecondsAbsoluteFailed");

    assert_buy_now_unavailable(&mut h);

    h.set_time(END_TIME)?;
    let _ = h.end()?;

    Ok(())
}

#[test]
fn buy_now_follows_bid_timing() -> Result<()> {
    // Without a grace period, the auction can't be bought after the end time
    let mut h = Harness::new(with_buy_now(BUY_NOW_PRICE))?;
    h.set_time(END_TIME)?;
    assert_fails_with(h.buy_now(false), "AssertBeforeSecondsAbsoluteFailed");
    assert_fails_with(h.buy_now(true), "AssertBeforeSecondsAbsoluteFailed");

    // With a grace period, it can be bought within the grace period of the last bid
    let mut h = Harness::new(Config {
        timings: Timings::new(END_TIME, 100),
        ..with_buy_now(BUY_NOW_PRICE)
    })?;
    h.set_time(950)?;
    h.bid(100)?;

    h.set_time(1_020)?;
    assert_fails_with(h.buy_now(false), "AssertBeforeSecondsAbsoluteFailed");
    let (buyer, nft) = h.buy_now(true)?;
    assert_eq!(nft.info.p2_puzzle_hash, buyer.bid.puzzle_hash);
    assert!(h.auction.info.state.ended);

    Ok(())
}

#[test]
fn seller_can_cancel_before_a_buy_now() -> Result<()> {
    let mut h = Harness::new(with_buy_now(BUY_NOW_PRICE))?;

    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
    assert_eq!(h.auction.info.available_buy_now_price(), None);
    assert_buy_now_unavailable(&mut h);

    Ok(())
}

#[test]
fn buy_now_price_round_trips_through_the_memo() -> Result<()> {
    // The harness parses the launcher spend and compares it to the launched auction
    let h = Harness::new(with_buy_now(BUY_NOW_PRICE))?;
    assert_eq!(
        h.auction.info.memo().settings.buy_now_price,
        Some(BUY_NOW_PRICE)
    );

    // The buy now price is curried into the auction, so the same settings without it are a
    // different auction
    let mut settings = h.auction.info.settings;
    settings.buy_now_price = None;
    let info = AuctionInfo {
        settings,
        ..h.auction.info
    };
    assert_ne!(info.inner_puzzle_hash(), h.auction.info.inner_puzzle_hash());

    Ok(())
}
