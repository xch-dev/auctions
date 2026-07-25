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
