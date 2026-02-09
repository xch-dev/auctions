use chia_wallet_sdk::{clvm_traits::apply_constants, prelude::*};

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
    pub minimum_bid: u64,
    pub bid_increment: u64,
    pub timings: Timings,
    pub payments: Payments,
}

#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionMemo {
    #[clvm(constant = 0)]
    pub version: u8,
    #[clvm(rest)]
    pub settings: AuctionSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionState {
    pub reserve_amount: u64,
    #[clvm(rest)]
    pub winning_bid: Bid,
}

impl AuctionState {
    pub fn initial(p2_puzzle_hash: Bytes32) -> Self {
        Self {
            reserve_amount: 0,
            winning_bid: Bid::new(0, p2_puzzle_hash),
        }
    }
}

pub fn calculate_bps_payment(bid_amount: u64, bps: u64) -> u64 {
    bid_amount * bps / 10000
}
