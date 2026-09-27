mod common;

use anyhow::Result;
use auction::*;
use chia_wallet_sdk::{
    clvm_traits::{clvm_list, clvm_quote},
    prelude::*,
};
use common::*;

#[test]
fn full_auction_pays_everyone() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let first = h.bid(100)?;
    assert_eq!(first.reserve_amount, 100 + 3 + 5);
    assert_eq!(h.auction.info.reserve.coin().amount, first.reserve_amount);

    let outbid_reserve = h.auction.info.reserve;
    let second = h.bid(200)?;
    assert_eq!(second.reserve_amount, 200 + 6 + 10);
    h.assert_unspent(h.reserve_output(outbid_reserve, first.bid.puzzle_hash, first.reserve_amount));

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_unspent(h.reserve_output(final_reserve, PREMIUM_PUZZLE_HASH, 6));
    h.assert_unspent(h.reserve_output(final_reserve, COMMISSION_PUZZLE_HASH, 2));
    h.assert_unspent(h.reserve_output(final_reserve, h.seller.puzzle_hash, 198));
    h.assert_unspent(h.royalty_output(final_reserve, 10));
    h.assert_unspent(h.auction.info.reserve.coin());
    assert_eq!(h.auction.info.reserve.coin().amount, 0);

    h.assert_unspent(nft.coin);
    assert_eq!(nft.info.p2_puzzle_hash, second.bid.puzzle_hash);
    assert_eq!(nft.info.current_owner, None);

    assert!(h.auction.info.state.ended);
    assert_eq!(h.auction.info.state.winning_bid, second.bid);

    Ok(())
}

#[test]
fn percent_bid_verifier() -> Result<()> {
    let mut h = Harness::new(Config {
        bid_verifier: BidVerifier::Percent {
            minimum_bid: 1_000,
            bid_increment_bps: 1_000,
        },
        ..Config::default()
    })?;

    assert_fails_with(h.bid(999), "raise");
    h.bid(1_000)?;
    assert_fails_with(h.bid(1_099), "raise");
    h.bid(1_100)?;
    assert_fails_with(h.bid(1_209), "raise");
    let winner = h.bid(1_210)?;

    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn no_bids_returns_nft_to_seller() -> Result<()> {
    for royalty_bps in [0, 500] {
        for reserve in [ReserveKind::Xch, ReserveKind::Cat] {
            let mut h = Harness::new(Config {
                royalty_bps,
                reserve,
                ..Config::default()
            })?;

            h.set_time(END_TIME)?;
            let final_reserve = h.auction.info.reserve;
            let nft = h.end()?;

            h.assert_unspent(nft.coin);
            assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
            assert_eq!(nft.info.current_owner, None);
            h.assert_missing(h.reserve_output(final_reserve, h.seller.puzzle_hash, 0));
        }
    }

    Ok(())
}

#[test]
fn zero_fee_auction() -> Result<()> {
    let mut h = Harness::new(Config {
        buyers_premium_bps: 0,
        commission_bps: 0,
        royalty_bps: 0,
        ..Config::default()
    })?;

    let winner = h.bid(100)?;
    assert_eq!(winner.reserve_amount, 100);

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_unspent(h.reserve_output(final_reserve, h.seller.puzzle_hash, 100));
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn commission_of_the_entire_bid() -> Result<()> {
    let mut h = Harness::new(Config {
        commission_bps: 10_000,
        ..Config::default()
    })?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;

    h.assert_unspent(h.reserve_output(final_reserve, COMMISSION_PUZZLE_HASH, 100));
    h.assert_missing(h.reserve_output(final_reserve, h.seller.puzzle_hash, 0));

    Ok(())
}

#[test]
fn multiple_bids_in_one_spend() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let first = h.bid(100)?;
    let reserve_before = h.auction.info.reserve;
    let bidders = h.bids(&[(200, false), (300, false)])?;

    h.assert_unspent(h.reserve_output(reserve_before, first.bid.puzzle_hash, first.reserve_amount));
    h.assert_unspent(h.reserve_output(
        reserve_before,
        bidders[0].bid.puzzle_hash,
        bidders[0].reserve_amount,
    ));
    assert_eq!(h.auction.info.state.winning_bid, bidders[1].bid);
    assert_eq!(
        h.auction.info.reserve.coin().amount,
        bidders[1].reserve_amount
    );

    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, bidders[1].bid.puzzle_hash);

    Ok(())
}

#[test]
fn nft_amount_must_match() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let winner = h.bid(100)?;
    h.set_time(END_TIME)?;

    // Claim the 1 mojo NFT is worth 3 mojos in both the end action and the unlock
    let wrong_amount = 3u64;
    let ctx = &mut h.ctx;

    let end_puzzle = h.auction.info.end_action(ctx)?;
    let end_solution = ctx.alloc(&clvm_list!(wrong_amount))?;
    let end_action = Spend::new(end_puzzle, end_solution);

    let unlocker = h.auction.info.nft_unlocker(ctx)?;
    let unlocker_solution = ctx.alloc(&NftUnlockerSolution::new(winner.bid, wrong_amount))?;
    let conditions = ctx.run(unlocker, unlocker_solution)?;
    let (assert_my_amount, _) = ctx.extract::<(AssertMyAmount, NodePtr)>(conditions)?;
    assert_eq!(assert_my_amount, AssertMyAmount::new(wrong_amount));
    let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
    let unlock = spend_auction_lock(
        ctx,
        h.auction.info.launcher_id,
        h.auction.info.inner_puzzle_hash(),
        Spend::new(delegated_puzzle, NodePtr::NIL),
    )?;
    let _ = h.nft.spend(ctx, unlock)?;

    let _ = h.auction.spend(ctx, vec![end_action], vec![])?;
    // The NFT puzzle itself also rejects a CREATE_COIN of 3, so this fails before AssertMyAmount is
    // checked, but the unlocker must not rely on that
    assert!(h.sim.spend_coins(ctx.take(), &[]).is_err());

    let nft = h.end()?;
    assert_eq!(nft.coin.amount, 1);
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn nft_with_did_owner_is_cleared() -> Result<()> {
    for royalty_bps in [0, 500] {
        let mut h = Harness::new(Config {
            royalty_bps,
            did_owner: true,
            ..Config::default()
        })?;
        assert!(h.nft.info.current_owner.is_some());

        let winner = h.bid(100)?;
        h.set_time(END_TIME)?;
        let nft = h.end()?;

        assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);
        assert_eq!(nft.info.current_owner, None);
    }

    Ok(())
}
