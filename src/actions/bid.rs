use chia_wallet_sdk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BidActionArgs {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BidActionSolution {}

compile_rue!(BidActionArgs = BID_ACTION, "puzzles/actions/bid_action.rue");
