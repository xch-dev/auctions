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
    AUCTION_SINGLETON_AMOUNT, Auction, AuctionError, AuctionInfo, AuctionMemo, BidVerifier,
    MAX_BPS, auction_lock_p2_puzzle_hash,
};

/// The spend of an auction's launcher coin, before it's been checked against the locked NFT.
///
/// The NFT's royalty info is curried into the auction, but isn't stored in the memo. So wallets
/// must fetch the NFT at [`nft_coin_id`](AuctionLaunch::nft_coin_id) from the blockchain, and pass
/// its royalty info to [`into_auction`](AuctionLaunch::into_auction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuctionLaunch {
    pub launcher_coin: Coin,
    pub memo: AuctionMemo,
    pub singleton_puzzle_hash: Bytes32,
    pub singleton_amount: u64,
}

impl AuctionLaunch {
    /// Returns [`None`] if the coin isn't a launcher, or its solution doesn't contain an auction memo.
    pub fn parse(
        allocator: &Allocator,
        launcher_coin: Coin,
        launcher_solution: NodePtr,
    ) -> Option<Self> {
        if launcher_coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
            return None;
        }

        let solution =
            LauncherSolution::<AuctionMemo>::from_clvm(allocator, launcher_solution).ok()?;

        Some(Self {
            launcher_coin,
            memo: solution.key_value_list,
            singleton_puzzle_hash: solution.singleton_puzzle_hash,
            singleton_amount: solution.amount,
        })
    }

    pub fn nft_coin_id(&self) -> Bytes32 {
        self.memo.nft_coin_id
    }

    /// Reconstructs the auction, given the royalty info of the NFT at
    /// [`nft_coin_id`](AuctionLaunch::nft_coin_id). Returns an error if the auction is invalid, or
    /// doesn't match the launched singleton, which includes a royalty that doesn't match the auction.
    ///
    /// Some things can't be checked offline, so wallets must also check the locked NFT and the
    /// reserve before bidding.
    pub fn into_auction(self, nft_royalty: RoyaltyInfo) -> Result<Auction, AuctionError> {
        let launcher_id = self.launcher_coin.coin_id();
        let info = AuctionInfo::from_memo(launcher_id, self.memo, nft_royalty);

        validate_auction(&info)?;

        if self.singleton_amount != AUCTION_SINGLETON_AMOUNT {
            return Err(AuctionError::InvalidSingletonAmount(self.singleton_amount));
        }

        let puzzle_hash: Bytes32 =
            SingletonArgs::curry_tree_hash(launcher_id, info.inner_puzzle_hash()).into();

        if puzzle_hash != self.singleton_puzzle_hash {
            return Err(AuctionError::PuzzleHashMismatch);
        }

        let coin = Coin::new(launcher_id, puzzle_hash, self.singleton_amount);

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: self.launcher_coin.parent_coin_info,
            parent_amount: self.launcher_coin.amount,
        });

        Ok(Auction::new(coin, proof, info))
    }
}

/// Rejects auctions that can't always be ended. [`launch_auction`](crate::AuctionLauncherExt::launch_auction)
/// runs the same checks, but the checks that are left to wallets, such as whether the NFT and
/// reserve are locked by the auction, are the seller's responsibility when launching.
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
