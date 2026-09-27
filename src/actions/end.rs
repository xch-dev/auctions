use chia_wallet_sdk::prelude::*;

use crate::{Payments, Timings, include_puzzle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EndActionArgs<U = NodePtr> {
    pub unlocker: U,
    pub timings: Timings,
    pub payments: Payments,
    pub royalty_bps: u64,
    /// The inner puzzle hash of the settlement coin created by the reserve, which is wrapped by the
    /// CAT layer for CAT reserves.
    pub settlement_payment_hash: Bytes32,
    pub nft_coin_id: Bytes32,
}

impl<U> EndActionArgs<U> {
    pub fn new(
        unlocker: U,
        timings: Timings,
        payments: Payments,
        royalty_bps: u64,
        settlement_payment_hash: Bytes32,
        nft_coin_id: Bytes32,
    ) -> Self {
        Self {
            unlocker,
            timings,
            payments,
            royalty_bps,
            settlement_payment_hash,
            nft_coin_id,
        }
    }
}

include_puzzle!(EndActionArgs<U> = END_ACTION, "puzzles/actions/end_action.rue");
