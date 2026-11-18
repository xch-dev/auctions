use chia_wallet_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Bid {
    pub amount: u64,
    #[clvm(rest)]
    pub puzzle_hash: Bytes32,
}

impl Bid {
    pub fn new(amount: u64, puzzle_hash: Bytes32) -> Self {
        Self {
            amount,
            puzzle_hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Timings {
    pub end_time: u64,
    #[clvm(rest)]
    pub grace_period: u64,
}

impl Timings {
    pub fn new(end_time: u64, grace_period: u64) -> Self {
        Self {
            end_time,
            grace_period,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BpsPayment {
    pub bps: u64,
    #[clvm(rest)]
    pub puzzle_hash: Bytes32,
}

impl BpsPayment {
    pub fn new(bps: u64, puzzle_hash: Bytes32) -> Self {
        Self { bps, puzzle_hash }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct Payments {
    pub buyers_premium: BpsPayment,
    pub commission: BpsPayment,
    #[clvm(rest)]
    pub payout_puzzle_hash: Bytes32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionSettings {
    pub bid_verifier: BidVerifier,
    pub timings: Timings,
    pub payments: Payments,
    /// The price at which a bid wins the auction outright and ends it right away, if any. No bid can
    /// exceed it. It must be at least the minimum bid, and can't be `Some(0)`.
    pub buy_now_price: Option<u64>,
}

impl AuctionSettings {
    /// The buy now price as it's curried into the bid and end actions, where 0 means there isn't one.
    pub fn buy_now_price_arg(&self) -> u64 {
        self.buy_now_price.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
pub enum BidVerifier {
    Flat {
        minimum_bid: u64,
        bid_increment: u64,
    } = 0,
    Percent {
        minimum_bid: u64,
        bid_increment_bps: u64,
    } = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionState {
    pub reserve_amount: u64,
    pub ended: bool,
    #[clvm(rest)]
    pub winning_bid: Bid,
}

impl AuctionState {
    pub fn initial(p2_puzzle_hash: Bytes32) -> Self {
        Self {
            reserve_amount: 0,
            ended: false,
            winning_bid: Bid::new(0, p2_puzzle_hash),
        }
    }
}

pub const MAX_BPS: u64 = 10_000;

/// The reserve finalizer always recreates the auction singleton with an amount of 1, so auctions
/// must be launched with it too.
pub const AUCTION_SINGLETON_AMOUNT: u64 = 1;

/// Rounds down, matching the puzzles. Returns [`None`] if the result doesn't fit in a [`u64`], which
/// can only happen if `bps` is greater than [`MAX_BPS`].
pub fn calculate_bps_payment(amount: u64, bps: u64) -> Option<u64> {
    u64::try_from(u128::from(amount) * u128::from(bps) / u128::from(MAX_BPS)).ok()
}
