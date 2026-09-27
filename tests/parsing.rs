mod common;

use anyhow::{Result, anyhow};
use auction::*;
use chia_wallet_sdk::{chia::puzzle_types::singleton::LauncherSolution, prelude::*};
use common::*;

fn launcher_spend(h: &Harness, launcher_id: Bytes32) -> Result<CoinSpend> {
    h.sim
        .coin_spend(launcher_id)
        .ok_or(anyhow!("missing launcher spend"))
}

/// Parses the auction launcher with a modified launcher solution.
fn parse_modified(
    h: &mut Harness,
    modify: impl FnOnce(&mut LauncherSolution<AuctionMemo>),
) -> Result<Option<Auction>, AuctionError> {
    let spend = launcher_spend(h, h.auction.info.launcher_id).unwrap();
    let solution = spend.solution.to_clvm(&mut h.ctx).unwrap();
    let mut solution = h
        .ctx
        .extract::<LauncherSolution<AuctionMemo>>(solution)
        .unwrap();
    modify(&mut solution);
    let solution = h.ctx.alloc(&solution).unwrap();
    parse_auction_launch(&h.ctx, spend.coin, solution)
}

#[test]
fn parses_every_reserve_kind() -> Result<()> {
    // The harness parses every launch and spend, so this just covers each reserve kind end to end
    for reserve in [
        ReserveKind::Xch,
        ReserveKind::Cat,
        ReserveKind::RevocableCat,
    ] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;
        h.bids(&[(100, false), (200, false)])?;
        h.set_time(END_TIME)?;
        let _ = h.end()?;
    }

    Ok(())
}

#[test]
fn ignores_other_launchers() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let spend = launcher_spend(&h, h.nft.info.launcher_id)?;
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    assert_eq!(parse_auction_launch(&h.ctx, spend.coin, solution)?, None);

    // The right memo, but not spent by a launcher
    let spend = launcher_spend(&h, h.auction.info.launcher_id)?;
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    let not_a_launcher = Coin::new(
        spend.coin.parent_coin_info,
        h.seller.puzzle_hash,
        spend.coin.amount,
    );
    assert_eq!(
        parse_auction_launch(&h.ctx, not_a_launcher, solution)?,
        None
    );

    Ok(())
}

#[test]
fn rejects_memos_that_dont_match_the_singleton() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let result = parse_modified(&mut h, |solution| {
        solution.key_value_list.settings.timings.end_time += 1;
    });
    assert!(matches!(result, Err(AuctionError::PuzzleHashMismatch)));

    let result = parse_modified(&mut h, |solution| {
        solution.key_value_list.nft.coin_id = Bytes32::new([7; 32]);
    });
    assert!(matches!(result, Err(AuctionError::PuzzleHashMismatch)));

    let result = parse_modified(&mut h, |solution| {
        solution.key_value_list.nft.royalty_basis_points += 1;
    });
    assert!(matches!(result, Err(AuctionError::PuzzleHashMismatch)));

    Ok(())
}

#[test]
fn rejects_even_singleton_amounts() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let result = parse_modified(&mut h, |solution| solution.amount = 2);
    assert!(matches!(result, Err(AuctionError::EvenSingletonAmount(2))));

    Ok(())
}

#[test]
fn follows_the_reserve_spent_by_someone_else() -> Result<()> {
    for reserve in [
        ReserveKind::Xch,
        ReserveKind::Cat,
        ReserveKind::RevocableCat,
    ] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;
        let tracked = h.auction;

        // Someone else bids while spending a decoy as the reserve, so the tracked reserve is left behind
        h.auction.info.reserve = h.decoy_reserve()?;
        let _ = h.bid(100)?;
        h.assert_unspent(tracked.info.reserve.coin());

        let spend = h
            .sim
            .coin_spend(tracked.coin.coin_id())
            .ok_or(anyhow!("missing auction spend"))?;
        let solution = spend.solution.to_clvm(&mut h.ctx)?;
        assert_eq!(tracked.parse_child(&mut h.ctx, solution)?, h.auction);

        // The auction continues from the decoy's lineage
        let _ = h.bid(200)?;
        h.set_time(END_TIME)?;
        let _ = h.end()?;
    }

    Ok(())
}

#[test]
fn rejects_invalid_auctions() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    // Even if the singleton was launched with these settings, the auction is invalid
    let result = parse_modified(&mut h, |solution| {
        solution.key_value_list.settings.bid_verifier = BidVerifier::Flat {
            minimum_bid: 0,
            bid_increment: 100,
        };
    });
    assert!(matches!(result, Err(AuctionError::ZeroMinimumBid)));

    let result = parse_modified(&mut h, |solution| {
        solution
            .key_value_list
            .settings
            .payments
            .commission
            .puzzle_hash = PREMIUM_PUZZLE_HASH;
    });
    assert!(matches!(
        result,
        Err(AuctionError::DuplicatePaymentPuzzleHash(_))
    ));

    Ok(())
}
