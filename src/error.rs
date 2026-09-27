use chia_wallet_sdk::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuctionError {
    #[error("driver error: {0}")]
    Driver(#[from] DriverError),

    #[error("the minimum bid must be greater than 0")]
    ZeroMinimumBid,

    #[error("the bid increment must be greater than 0")]
    ZeroBidIncrement,

    #[error("the {name} of {bps} bps exceeds 10,000 bps")]
    BpsTooHigh { name: &'static str, bps: u64 },

    #[error("puzzle hash {0} receives more than one payment when the auction ends")]
    DuplicatePaymentPuzzleHash(Bytes32),

    #[error("puzzle hash {0} can't receive a payment, since it's reserved by the auction")]
    ReservedPaymentPuzzleHash(Bytes32),

    #[error("the launched singleton doesn't match the auction memo")]
    PuzzleHashMismatch,

    #[error("the auction singleton has an amount of {0}, but it must be 1")]
    InvalidSingletonAmount(u64),
}
