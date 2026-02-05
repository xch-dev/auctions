use std::borrow::Cow;

use chia_wallet_sdk::prelude::*;

use crate::{Bid, Timings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BidActionArgs<V = NodePtr> {
    pub bid_verifier: V,
    pub timings: Timings,
    pub buyers_premium_bps: u64,
}

impl<V> BidActionArgs<V> {
    pub fn new(bid_verifier: V, timings: Timings, buyers_premium_bps: u64) -> Self {
        Self {
            bid_verifier,
            timings,
            buyers_premium_bps,
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

struct BidActionMod;

compile_rue!(BidActionMod = BID_ACTION, "puzzles/actions/bid_action.rue");

impl<V> Mod for BidActionArgs<V> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        BidActionMod::mod_reveal()
    }

    fn mod_hash() -> TreeHash {
        BidActionMod::mod_hash()
    }
}
