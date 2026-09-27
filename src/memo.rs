use chia_wallet_sdk::{chia::puzzle_types::LineageProof, clvm_traits::apply_constants, prelude::*};

use crate::AuctionSettings;

/// Stored in the launcher solution's key value list. It contains everything needed to reconstruct
/// the auction from the launcher spend.
#[apply_constants]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionMemo {
    #[clvm(constant = 0)]
    pub version: u8,
    pub settings: AuctionSettings,
    pub nft: NftMemo,
    #[clvm(rest)]
    pub reserve: ReserveMemo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NftMemo {
    pub coin_id: Bytes32,
    pub launcher_id: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub royalty_basis_points: u16,
}

/// The reserve is always an empty coin locked by the auction when it's launched, so only its parent
/// (and asset, for CATs) is needed to find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
pub enum ReserveMemo {
    Xch {
        parent_coin_info: Bytes32,
    } = 0,
    Cat {
        parent_coin_info: Bytes32,
        lineage_proof: Option<LineageProof>,
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
    } = 1,
}
