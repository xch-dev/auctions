use chia_wallet_sdk::prelude::*;

use crate::{Payments, Timings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EndActionArgs<U = NodePtr> {
    pub unlocker: U,
    pub timings: Timings,
    pub payments: Payments,
    pub nft_coin_id: Bytes32,
    pub settlement_payment_hash: Option<Bytes32>,
}

impl<U> EndActionArgs<U> {
    pub fn new(
        unlocker: U,
        timings: Timings,
        payments: Payments,
        nft_coin_id: Bytes32,
        settlement_payment_hash: Option<Bytes32>,
    ) -> Self {
        Self {
            unlocker,
            timings,
            payments,
            nft_coin_id,
            settlement_payment_hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct EndActionSolution {
    pub nft_amount: u64,
}

impl EndActionSolution {
    pub fn new(nft_amount: u64) -> Self {
        Self { nft_amount }
    }
}

compile_rue!(
    debug EndActionArgs<U> = END_ACTION,
    "puzzles/actions/end_action.rue"
);
