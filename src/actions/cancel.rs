use chia_wallet_sdk::prelude::*;

use crate::include_puzzle;

/// The message the auction's payout puzzle must send to the auction coin to authorize a cancel.
pub const CANCEL_MESSAGE: &[u8] = b"cancel";

/// The message mode used to authorize a cancel, which commits to the sender's puzzle hash and the
/// receiving auction coin.
pub const CANCEL_MESSAGE_MODE: u8 = 0b010_111;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct CancelActionArgs<U = NodePtr> {
    pub unlocker: U,
    pub payout_puzzle_hash: Bytes32,
    pub nft_coin_id: Bytes32,
}

impl<U> CancelActionArgs<U> {
    pub fn new(unlocker: U, payout_puzzle_hash: Bytes32, nft_coin_id: Bytes32) -> Self {
        Self {
            unlocker,
            payout_puzzle_hash,
            nft_coin_id,
        }
    }
}

include_puzzle!(
    CancelActionArgs<U> = CANCEL_ACTION,
    "puzzles/actions/cancel_action.rue"
);
