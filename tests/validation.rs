mod common;

use anyhow::Result;
use auction::*;
use chia_wallet_sdk::{prelude::*, puzzles::SETTLEMENT_PAYMENT_HASH};
use common::*;

fn launch_error(
    config: Config,
    settings: impl FnOnce(&Config, Bytes32, Bytes32) -> AuctionSettings,
) -> AuctionError {
    match Harness::with_settings(config, settings) {
        Ok(_) => panic!("expected the auction to be rejected"),
        Err(error) => error
            .downcast::<AuctionError>()
            .expect("expected an auction error"),
    }
}

fn with_verifier(bid_verifier: BidVerifier) -> Config {
    Config {
        bid_verifier,
        ..Config::default()
    }
}

fn with_payout(
    payout_puzzle_hash: Bytes32,
) -> impl FnOnce(&Config, Bytes32, Bytes32) -> AuctionSettings {
    move |config, _, _| config.settings(payout_puzzle_hash)
}

fn default_settings(config: &Config, seller: Bytes32, _: Bytes32) -> AuctionSettings {
    config.settings(seller)
}

#[test]
fn rejects_zero_minimum_bid() {
    for bid_verifier in [
        BidVerifier::Flat {
            minimum_bid: 0,
            bid_increment: 100,
        },
        BidVerifier::Percent {
            minimum_bid: 0,
            bid_increment_bps: 100,
        },
    ] {
        let error = launch_error(with_verifier(bid_verifier), default_settings);
        assert!(matches!(error, AuctionError::ZeroMinimumBid), "{error}");
    }
}

#[test]
fn rejects_zero_bid_increment() {
    for bid_verifier in [
        BidVerifier::Flat {
            minimum_bid: 100,
            bid_increment: 0,
        },
        BidVerifier::Percent {
            minimum_bid: 100,
            bid_increment_bps: 0,
        },
    ] {
        let error = launch_error(with_verifier(bid_verifier), default_settings);
        assert!(matches!(error, AuctionError::ZeroBidIncrement), "{error}");
    }
}

#[test]
fn rejects_fees_over_100_percent() {
    let configs = [
        Config {
            buyers_premium_bps: 10_001,
            ..Config::default()
        },
        Config {
            commission_bps: 10_001,
            ..Config::default()
        },
        Config {
            royalty_bps: 10_001,
            ..Config::default()
        },
    ];

    for config in configs {
        let error = launch_error(config, default_settings);
        assert!(
            matches!(error, AuctionError::BpsTooHigh { bps: 10_001, .. }),
            "{error}"
        );
    }
}

#[test]
fn allows_fees_of_100_percent() -> Result<()> {
    let mut h = Harness::new(Config {
        buyers_premium_bps: 10_000,
        commission_bps: 10_000,
        royalty_bps: 10_000,
        ..Config::default()
    })?;

    let winner = h.bid(100)?;
    assert_eq!(winner.reserve_amount, 300);

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;
    h.assert_settled(final_reserve, 100);

    Ok(())
}

#[test]
fn rejects_duplicate_payment_puzzle_hashes() {
    let settings: [fn(&Config, Bytes32, Bytes32) -> AuctionSettings; 3] = [
        |config, seller, _| {
            let mut settings = config.settings(seller);
            settings.payments.buyers_premium.puzzle_hash = COMMISSION_PUZZLE_HASH;
            settings
        },
        |config, _, _| config.settings(PREMIUM_PUZZLE_HASH),
        |config, _, _| config.settings(COMMISSION_PUZZLE_HASH),
    ];

    for settings in settings {
        let error = launch_error(Config::default(), settings);
        assert!(
            matches!(error, AuctionError::DuplicatePaymentPuzzleHash(_)),
            "{error}"
        );
    }
}

#[test]
fn allows_shared_puzzle_hash_for_unpaid_fees() -> Result<()> {
    let config = Config {
        buyers_premium_bps: 0,
        commission_bps: 0,
        ..Config::default()
    };

    let mut h = Harness::with_settings(config, |config, seller, _| {
        let mut settings = config.settings(seller);
        settings.payments.buyers_premium.puzzle_hash = seller;
        settings.payments.commission.puzzle_hash = seller;
        settings
    })?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;
    h.assert_unspent(h.reserve_output(final_reserve, h.seller.puzzle_hash, 100));

    Ok(())
}

#[test]
fn rejects_reserved_payment_puzzle_hashes() {
    let error = launch_error(
        Config::default(),
        with_payout(SETTLEMENT_PAYMENT_HASH.into()),
    );
    assert!(
        matches!(error, AuctionError::ReservedPaymentPuzzleHash(_)),
        "{error}"
    );

    let error = launch_error(Config::default(), |config, _, lock_puzzle_hash| {
        config.settings(lock_puzzle_hash)
    });
    assert!(
        matches!(error, AuctionError::ReservedPaymentPuzzleHash(_)),
        "{error}"
    );

    let error = launch_error(
        Config {
            reserve: ReserveKind::Cat,
            ..Config::default()
        },
        |config, seller, lock_puzzle_hash| {
            let mut settings = config.settings(seller);
            settings.payments.commission.puzzle_hash = lock_puzzle_hash;
            settings
        },
    );
    assert!(
        matches!(error, AuctionError::ReservedPaymentPuzzleHash(_)),
        "{error}"
    );
}

#[test]
fn bps_math_does_not_overflow() -> Result<()> {
    assert_eq!(calculate_bps_payment(u64::MAX, MAX_BPS), Some(u64::MAX));
    assert_eq!(calculate_bps_payment(u64::MAX, 5_000), Some(u64::MAX / 2));
    assert_eq!(calculate_bps_payment(u64::MAX, MAX_BPS + 1), None);
    assert_eq!(calculate_bps_payment(19, 500), Some(0));
    assert_eq!(calculate_bps_payment(20, 500), Some(1));

    let h = Harness::new(Config::default())?;
    assert_eq!(h.auction.info.reserve_amount_for_bid(u64::MAX), None);
    assert_eq!(h.auction.info.reserve_amount_for_bid(100), Some(108));

    Ok(())
}
