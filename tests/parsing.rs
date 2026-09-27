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

/// Parses the auction launcher with a modified launcher solution, and the locked NFT's royalty.
fn parse_modified(
    h: &mut Harness,
    modify: impl FnOnce(&mut LauncherSolution<AuctionMemo>),
) -> Result<Auction, AuctionError> {
    let royalty = nft_royalty(&h.nft);
    parse_with_royalty(h, modify, royalty)
}

fn parse_with_royalty(
    h: &mut Harness,
    modify: impl FnOnce(&mut LauncherSolution<AuctionMemo>),
    royalty: RoyaltyInfo,
) -> Result<Auction, AuctionError> {
    let spend = launcher_spend(h, h.auction.info.launcher_id).unwrap();
    let solution = spend.solution.to_clvm(&mut h.ctx).unwrap();
    let mut solution = h
        .ctx
        .extract::<LauncherSolution<AuctionMemo>>(solution)
        .unwrap();
    modify(&mut solution);
    let solution = h.ctx.alloc(&solution).unwrap();
    AuctionLaunch::parse(&h.ctx, spend.coin, solution)
        .expect("expected an auction launch")
        .into_auction(royalty)
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
    assert_eq!(AuctionLaunch::parse(&h.ctx, spend.coin, solution), None);

    // The right memo, but not spent by a launcher
    let spend = launcher_spend(&h, h.auction.info.launcher_id)?;
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    let not_a_launcher = Coin::new(
        spend.coin.parent_coin_info,
        h.seller.puzzle_hash,
        spend.coin.amount,
    );
    assert_eq!(AuctionLaunch::parse(&h.ctx, not_a_launcher, solution), None);

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
        solution.key_value_list.nft_coin_id = Bytes32::new([7; 32]);
    });
    assert!(matches!(result, Err(AuctionError::PuzzleHashMismatch)));

    Ok(())
}

#[test]
fn rejects_royalties_that_dont_match_the_singleton() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    // The royalty basis points are curried into the auction, so an NFT with a different royalty
    // than the one the auction was launched with is rejected
    let mut royalty = nft_royalty(&h.nft);
    royalty.basis_points += 1;
    let result = parse_with_royalty(&mut h, |_| {}, royalty);
    assert!(matches!(result, Err(AuctionError::PuzzleHashMismatch)));

    Ok(())
}

#[test]
fn rejects_singleton_amounts_other_than_1() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    for amount in [2, 3] {
        let result = parse_modified(&mut h, |solution| solution.amount = amount);
        assert!(
            matches!(result, Err(AuctionError::InvalidSingletonAmount(a)) if a == amount),
            "{result:?}"
        );
    }

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
