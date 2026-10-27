mod common;

use anyhow::{Result, anyhow};
use auction::*;
use chia_wallet_sdk::prelude::*;
use common::*;

#[test]
fn cancel_returns_nft_to_seller() -> Result<()> {
    for reserve in [ReserveKind::Xch, ReserveKind::Cat] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        let reserve_before = h.auction.info.reserve;
        let nft = h.cancel()?;

        h.assert_unspent(nft.coin);
        assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
        assert_eq!(nft.info.current_owner, None);

        assert!(h.auction.info.state.ended);
        assert_eq!(h.auction.info.state.reserve_amount, 0);
        assert_eq!(
            h.auction.info.state.winning_bid,
            Bid::new(0, h.seller.puzzle_hash)
        );

        // Nothing is paid out, and the reserve is recreated empty
        h.assert_missing(h.reserve_output(reserve_before, h.seller.puzzle_hash, 0));
        let child_reserve = h.auction.info.reserve.coin();
        assert_eq!(
            child_reserve.parent_coin_info,
            reserve_before.coin().coin_id()
        );
        assert_eq!(child_reserve.amount, 0);
        h.assert_unspent(child_reserve);

        // A cancelled auction can't be bid on, ended, or cancelled again
        assert_fails_with(h.bid(100), "raise");
        h.set_time(END_TIME)?;
        assert_fails_with(h.end(), "raise");
        assert_fails_with(h.cancel(), "raise");
    }

    Ok(())
}

#[test]
fn cancel_requires_the_sellers_message() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    assert_fails_with(h.cancel_authorized_by(None), "MessageNotSentOrReceived");

    // A message from any puzzle other than the payout puzzle doesn't authorize the cancel
    let stranger = h.sim.bls(0);
    assert_fails_with(
        h.cancel_authorized_by(Some(stranger)),
        "MessageNotSentOrReceived",
    );

    // The auction is untouched, so the seller can still cancel it
    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn cannot_cancel_after_a_bid() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let winner = h.bid(100)?;

    assert_fails_with(h.cancel(), "raise");

    // The bid is still live and wins the auction
    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn cannot_cancel_after_the_auction_ended() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.set_time(END_TIME)?;
    let nft = h.end()?;

    assert_fails_with(h.cancel(), "raise");
    h.assert_unspent(nft.coin);

    Ok(())
}

#[test]
fn cancel_has_no_timing_restrictions() -> Result<()> {
    // Well before the end time, and with a grace period that would otherwise delay the end
    let mut h = Harness::new(Config {
        timings: Timings::new(END_TIME, 500),
        ..Config::default()
    })?;
    assert!(h.sim.next_timestamp() < END_TIME);

    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn parse_child_follows_a_cancel() -> Result<()> {
    for reserve in [ReserveKind::Xch, ReserveKind::Cat] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        let before = h.auction;
        let _ = h.cancel()?;

        // The harness already compares the parsed child to the spent one, but check it explicitly
        let coin_spend = h
            .sim
            .coin_spend(before.coin.coin_id())
            .ok_or(anyhow!("missing auction spend"))?;
        let solution = coin_spend.solution.to_clvm(&mut h.ctx)?;
        let parsed = before.parse_child(&mut h.ctx, solution)?;

        assert_eq!(parsed, h.auction);
        assert!(parsed.info.state.ended);
        assert_eq!(parsed.info.state.reserve_amount, 0);
        h.assert_unspent(parsed.coin);
        h.assert_unspent(parsed.info.reserve.coin());
    }

    Ok(())
}
