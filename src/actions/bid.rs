use chia_wallet_sdk::prelude::*;

use crate::{Bid, Timings, include_puzzle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BidActionArgs<V = NodePtr> {
    pub bid_verifier: V,
    pub timings: Timings,
    pub buyers_premium_bps: u64,
    pub royalty_bps: u64,
}

impl<V> BidActionArgs<V> {
    pub fn new(
        bid_verifier: V,
        timings: Timings,
        buyers_premium_bps: u64,
        royalty_bps: u64,
    ) -> Self {
        Self {
            bid_verifier,
            timings,
            buyers_premium_bps,
            royalty_bps,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BidActionSolution {
    pub bid: Bid,
    pub grace: bool,
}

impl BidActionSolution {
    pub fn new(bid: Bid, grace: bool) -> Self {
        Self { bid, grace }
    }
}

include_puzzle!(BidActionArgs<V> = BID_ACTION, "puzzles/actions/bid_action.rue");
