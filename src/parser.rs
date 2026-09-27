use std::collections::HashSet;

use chia_wallet_sdk::{
    chia::puzzle_types::{
        EveProof, Proof,
        singleton::{LauncherSolution, SingletonArgs},
    },
    prelude::*,
    puzzles::{SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH},
};

use crate::{
    Auction, AuctionError, AuctionInfo, AuctionMemo, BidVerifier, MAX_BPS,
    auction_lock_p2_puzzle_hash,
};

/// Parses a newly launched auction from the spend of its launcher coin.
///
/// Returns [`None`] if the launcher isn't for an auction, and an error if it's for an invalid auction.
/// Some things can't be checked offline, so wallets must also check the locked NFT and the reserve
/// before bidding.
pub fn parse_auction_launch(
    allocator: &Allocator,
    launcher_coin: Coin,
    launcher_solution: NodePtr,
) -> Result<Option<Auction>, AuctionError> {
    if launcher_coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
        return Ok(None);
    }

    let Ok(solution) = LauncherSolution::<AuctionMemo>::from_clvm(allocator, launcher_solution)
    else {
        return Ok(None);
    };

    let launcher_id = launcher_coin.coin_id();
    let info = AuctionInfo::from_memo(launcher_id, solution.key_value_list);

    validate_auction(&info)?;

    let puzzle_hash: Bytes32 =
        SingletonArgs::curry_tree_hash(launcher_id, info.inner_puzzle_hash()).into();

    if puzzle_hash != solution.singleton_puzzle_hash {
        return Err(AuctionError::PuzzleHashMismatch);
    }

    if solution.amount % 2 == 0 {
        return Err(AuctionError::EvenSingletonAmount(solution.amount));
    }

    let coin = Coin::new(launcher_id, puzzle_hash, solution.amount);

    let proof = Proof::Eve(EveProof {
        parent_parent_coin_info: launcher_coin.parent_coin_info,
        parent_amount: launcher_coin.amount,
    });

    Ok(Some(Auction::new(coin, proof, info)))
}

/// Rejects auctions that can't always be ended. [`launch_auction`](crate::AuctionLauncherExt::launch_auction) runs the same checks, so
/// that sellers can't launch an auction that wallets would reject.
pub(crate) fn validate_auction(info: &AuctionInfo) -> Result<(), AuctionError> {
    let (minimum_bid, bid_increment) = match info.settings.bid_verifier {
        BidVerifier::Flat {
            minimum_bid,
            bid_increment,
        } => (minimum_bid, bid_increment),
        BidVerifier::Percent {
            minimum_bid,
            bid_increment_bps,
        } => (minimum_bid, bid_increment_bps),
    };

    if minimum_bid == 0 {
        return Err(AuctionError::ZeroMinimumBid);
    }

    if bid_increment == 0 {
        return Err(AuctionError::ZeroBidIncrement);
    }

    let payments = info.settings.payments;

    for (name, bps) in [
        ("buyer's premium", payments.buyers_premium.bps),
        ("commission", payments.commission.bps),
        ("NFT royalty", info.royalty_bps()),
    ] {
        if bps > MAX_BPS {
            return Err(AuctionError::BpsTooHigh { name, bps });
        }
    }

    let lock_puzzle_hash = auction_lock_p2_puzzle_hash(info.launcher_id);

    // The end action creates every payment from the reserve coin, so two payments with the same
    // puzzle hash could be identical coins, which would make the auction impossible to end.
    let mut puzzle_hashes = HashSet::new();

    for (is_paid, puzzle_hash) in [
        (
            payments.buyers_premium.bps > 0,
            payments.buyers_premium.puzzle_hash,
        ),
        (payments.commission.bps > 0, payments.commission.puzzle_hash),
        (true, payments.payout_puzzle_hash),
    ] {
        if !is_paid {
            continue;
        }

        if puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() || puzzle_hash == lock_puzzle_hash {
            return Err(AuctionError::ReservedPaymentPuzzleHash(puzzle_hash));
        }

        if !puzzle_hashes.insert(puzzle_hash) {
            return Err(AuctionError::DuplicatePaymentPuzzleHash(puzzle_hash));
        }
    }

    Ok(())
}
