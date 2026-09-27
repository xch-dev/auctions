mod common;

use anyhow::Result;
use auction::*;
use common::*;

fn with_grace(grace_period: u64) -> Config {
    Config {
        timings: Timings::new(END_TIME, grace_period),
        ..Config::default()
    }
}

#[test]
fn bids_rejected_after_end_time() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    h.set_time(END_TIME - 1)?;
    h.bid(100)?;

    h.set_time(END_TIME)?;
    assert_fails_with(h.bid(200), "AssertBeforeSecondsAbsoluteFailed");

    // Without a grace period, asking for the grace period falls back to the end time
    assert_fails_with(
        h.bid_with_grace(200, true),
        "AssertBeforeSecondsAbsoluteFailed",
    );

    Ok(())
}

#[test]
fn end_rejected_before_end_time() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.bid(100)?;

    h.set_time(END_TIME - 1)?;
    assert_fails_with(h.end(), "AssertSecondsAbsoluteFailed");

    h.set_time(END_TIME)?;
    let _ = h.end()?;

    Ok(())
}

#[test]
fn grace_period_extends_bidding() -> Result<()> {
    let mut h = Harness::new(with_grace(100))?;

    h.set_time(950)?;
    h.bid(100)?;

    // After the end time, only grace bids within 100 seconds of the last bid are allowed
    h.set_time(1_020)?;
    assert_fails_with(h.bid(200), "AssertBeforeSecondsAbsoluteFailed");
    let winner = h.bid_with_grace(200, true)?;

    // The auction can't end until the grace period after the last bid has passed
    h.set_time(1_100)?;
    assert_fails_with(h.end(), "AssertSecondsRelativeFailed");

    h.set_time(1_120)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn grace_window_closes() -> Result<()> {
    let mut h = Harness::new(with_grace(100))?;

    h.set_time(900)?;
    h.bid(100)?;

    h.set_time(1_000)?;
    assert_fails_with(
        h.bid_with_grace(200, true),
        "AssertBeforeSecondsRelativeFailed",
    );

    let _ = h.end()?;

    Ok(())
}

#[test]
fn bids_rejected_after_the_auction_ends() -> Result<()> {
    let mut h = Harness::new(with_grace(100))?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;
    assert!(h.auction.info.state.ended);

    // The grace window relative to the end spend is still open, so only the ended flag stops this
    h.set_time(END_TIME + 10)?;
    assert_fails_with(h.bid_with_grace(200, true), "raise");

    Ok(())
}

#[test]
fn auction_can_only_end_once() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;

    let end_action = h.auction.spend_end_action(&mut h.ctx, &h.nft)?;
    let result = h.auction.child_state(&mut h.ctx, &[end_action]);
    assert_fails_with(result.map_err(anyhow::Error::from), "raise");

    Ok(())
}
