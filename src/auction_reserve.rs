use chia_wallet_sdk::prelude::*;
use chia_wallet_sdk::puzzles::SETTLEMENT_PAYMENT_HASH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuctionReserve {
    Xch(Coin),
    Cat(Cat),
}

impl AuctionReserve {
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        match self {
            AuctionReserve::Xch(coin) => coin.puzzle_hash,
            AuctionReserve::Cat(cat) => cat.info.p2_puzzle_hash,
        }
    }

    pub fn coin(&self) -> Coin {
        match self {
            AuctionReserve::Xch(coin) => *coin,
            AuctionReserve::Cat(cat) => cat.coin,
        }
    }

    /// The same reserve with a different parent. The reserve finalizer takes the reserve's parent from
    /// the singleton's solution, so any coin with the reserve's puzzle hash and amount can be spent as
    /// the reserve.
    pub fn with_parent_coin_info(&self, parent_coin_info: Bytes32) -> Self {
        match self {
            AuctionReserve::Xch(coin) => {
                AuctionReserve::Xch(Coin::new(parent_coin_info, coin.puzzle_hash, coin.amount))
            }
            AuctionReserve::Cat(cat) => AuctionReserve::Cat(Cat {
                coin: Coin::new(parent_coin_info, cat.coin.puzzle_hash, cat.coin.amount),
                ..*cat
            }),
        }
    }

    /// The new reserve created when this reserve is spent.
    pub fn child(&self, amount: u64) -> Self {
        match self {
            AuctionReserve::Xch(coin) => {
                AuctionReserve::Xch(Coin::new(coin.coin_id(), coin.puzzle_hash, amount))
            }
            AuctionReserve::Cat(cat) => {
                AuctionReserve::Cat(cat.child(cat.info.p2_puzzle_hash, amount))
            }
        }
    }

    pub fn settlement_puzzle_hash(&self) -> Bytes32 {
        match self {
            AuctionReserve::Xch(_) => SETTLEMENT_PAYMENT_HASH.into(),
            AuctionReserve::Cat(cat) => CatInfo::new(
                cat.info.asset_id,
                cat.info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash()
            .into(),
        }
    }
}
