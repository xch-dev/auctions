use chia_wallet_sdk::prelude::*;

use crate::{Payments, Timings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct EndActionArgs {
    pub timings: Timings,
    pub payments: Payments,
    pub nft_coin_id: Bytes32,
}

impl EndActionArgs {
    pub fn new(timings: Timings, payments: Payments, nft_coin_id: Bytes32) -> Self {
        Self {
            timings,
            payments,
            nft_coin_id,
        }
    }
}

compile_rue!(EndActionArgs = END_ACTION, "puzzles/actions/end_action.rue");
