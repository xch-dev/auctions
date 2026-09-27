mod common;

use anyhow::Result;
use auction::*;
use chia_wallet_sdk::{prelude::*, types::announcement_id};
use common::*;

#[test]
fn minimum_bid_enforced() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    assert_fails_with(h.bid(99), "raise");
    h.bid(100)?;

    Ok(())
}

#[test]
fn flat_increment_enforced() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    h.bid(100)?;
    assert_fails_with(h.bid(199), "raise");
    h.bid(200)?;

    Ok(())
}

#[test]
fn bids_must_strictly_increase() -> Result<()> {
    // A 1% increment on a bid of 50 rounds down to 0, so the verifier alone would allow a tie
    let mut h = Harness::new(Config {
        bid_verifier: BidVerifier::Percent {
            minimum_bid: 1,
            bid_increment_bps: 100,
        },
        ..Config::default()
    })?;

    h.bid(50)?;
    assert_fails_with(h.bid(50), "raise");
    h.bid(51)?;

    Ok(())
}

#[test]
fn outbid_bidder_is_refunded_in_full() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let first = h.bid(1_000)?;
    let reserve = h.auction.info.reserve;
    h.bid(2_000)?;

    let refund = h.reserve_output(reserve, first.bid.puzzle_hash, first.reserve_amount);
    h.assert_unspent(refund);
    assert_eq!(refund.amount, 1_000 + 30 + 50);

    Ok(())
}

#[test]
fn malformed_bids_rejected() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let ctx = &mut h.ctx;

    let puzzle = h.auction.info.bid_action(ctx)?;
    let truth = ctx.alloc(&(NodePtr::NIL, h.auction.info.state))?;

    let valid = ctx.alloc(&(100u64, Bytes32::new([5; 32])))?;
    let short_puzzle_hash = ctx.alloc(&(100u64, Bytes::new(vec![5; 31])))?;
    let long_puzzle_hash = ctx.alloc(&(100u64, Bytes::new(vec![5; 33])))?;
    let pair_puzzle_hash = ctx.alloc(&(100u64, (Bytes32::new([5; 32]), ())))?;
    let zero_amount = ctx.alloc(&(0u64, Bytes32::new([5; 32])))?;
    let negative_amount = ctx.alloc(&(-100i64, Bytes32::new([5; 32])))?;
    let pair_amount = ctx.alloc(&((100u64, ()), Bytes32::new([5; 32])))?;

    let valid_solution = ctx.alloc(&(truth, (valid, (false, ()))))?;
    assert!(ctx.run(puzzle, valid_solution).is_ok());

    for bid in [
        short_puzzle_hash,
        long_puzzle_hash,
        pair_puzzle_hash,
        zero_amount,
        negative_amount,
        pair_amount,
    ] {
        let solution = ctx.alloc(&(truth, (bid, (false, ()))))?;
        assert!(ctx.run(puzzle, solution).is_err());

        // The driver runs the same puzzle, so it can't build a spend with a malformed bid either
        let action_solution = ctx.alloc(&(bid, (false, ())))?;
        let result = h
            .auction
            .child_state(ctx, &[Spend::new(puzzle, action_solution)]);
        assert!(result.is_err());
    }

    Ok(())
}

#[test]
fn non_canonical_bid_amount_is_normalized() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let reserve_amount = h.auction.info.reserve_amount_for_bid(100).unwrap();
    let bidder = h.sim.bls(reserve_amount);

    // 100 with a redundant leading zero byte
    let amount = h.ctx.new_atom(&[0x00, 0x64])?;
    let puzzle_hash = h.ctx.alloc(&bidder.puzzle_hash)?;
    let raw_bid = h.ctx.new_pair(amount, puzzle_hash)?;

    let puzzle = h.auction.info.bid_action(&mut h.ctx)?;
    let solution = h.ctx.alloc(&(raw_bid, (false, ())))?;

    // The announcement is for the canonical bid
    let bid = Bid::new(100, bidder.puzzle_hash);
    StandardLayer::new(bidder.pk).spend(
        &mut h.ctx,
        bidder.coin,
        Conditions::new()
            .assert_coin_announcement(announcement_id(h.auction.coin.coin_id(), bid.tree_hash())),
    )?;

    let auction = h
        .auction
        .spend(&mut h.ctx, vec![Spend::new(puzzle, solution)], vec![])?;
    h.sim.spend_coins(h.ctx.take(), &[bidder.sk])?;
    h.assert_unspent(auction.coin);
    assert_eq!(auction.info.state.winning_bid, bid);
    h.auction = auction;

    // The driver can still follow the auction
    let reserve = h.auction.info.reserve;
    let winner = h.bid(200)?;
    h.assert_unspent(h.reserve_output(reserve, bidder.puzzle_hash, reserve_amount));

    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn bid_announcement_commits_to_the_bid() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let reserve_amount = h.auction.info.reserve_amount_for_bid(100).unwrap();
    let bidder = h.sim.bls(reserve_amount);
    let bid = Bid::new(100, bidder.puzzle_hash);
    let action = h.auction.spend_bid_action(&mut h.ctx, bid, false)?;

    let other_bid = Bid::new(101, bidder.puzzle_hash);
    StandardLayer::new(bidder.pk).spend(
        &mut h.ctx,
        bidder.coin,
        Conditions::new().assert_coin_announcement(announcement_id(
            h.auction.coin.coin_id(),
            other_bid.tree_hash(),
        )),
    )?;

    let _ = h.auction.spend(&mut h.ctx, vec![action], vec![])?;
    let result = h.sim.spend_coins(h.ctx.take(), &[bidder.sk]);
    assert_fails_with(
        result.map_err(anyhow::Error::from),
        "AssertCoinAnnouncementFailed",
    );

    Ok(())
}
