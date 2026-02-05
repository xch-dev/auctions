use chia_wallet_sdk::{
    chia::puzzle_types::{EveProof, Proof, singleton::SingletonSolution},
    clvmr::serde::node_from_bytes,
    driver::{
        ActionLayer, ActionLayerSolution, Finalizer, InnerPuzzleSpend, MipsSpend, SingletonLayer,
        mips_puzzle_hash,
    },
    prelude::*,
    types::puzzles::{
        ActionLayerArgs, RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM,
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs, ReserveFinalizerSolution, SingletonMember,
        SingletonMemberSolution,
    },
};

use crate::{
    AuctionAsset, AuctionMemo, AuctionSettings, AuctionState, BidActionArgs, BidActionSolution,
    EndActionArgs, FlatBidVerifierArgs, calculate_bps_payment,
};

pub type Auction = Singleton<AuctionInfo>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuctionInfo {
    pub launcher_id: Bytes32,
    pub settings: AuctionSettings,
    pub asset: AuctionAsset,
    pub nft_coin_id: Bytes32,
    pub state: AuctionState,
}

impl AuctionInfo {
    pub fn new(
        launcher_id: Bytes32,
        settings: AuctionSettings,
        asset: AuctionAsset,
        nft_coin_id: Bytes32,
        state: AuctionState,
    ) -> Self {
        Self {
            launcher_id,
            settings,
            asset,
            nft_coin_id,
            state,
        }
    }

    fn bid_action(&self) -> BidActionArgs<FlatBidVerifierArgs> {
        BidActionArgs::new(
            FlatBidVerifierArgs::new(self.settings.minimum_bid, self.settings.bid_increment),
            self.settings.timings,
            self.settings.payments.buyers_premium.bps,
        )
    }

    fn end_action(&self) -> EndActionArgs {
        EndActionArgs::new(
            self.settings.timings,
            self.settings.payments,
            self.nft_coin_id,
        )
    }

    fn leaves(&self) -> [Bytes32; 2] {
        [
            self.bid_action().curry_tree_hash().into(),
            self.end_action().curry_tree_hash().into(),
        ]
    }

    fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&self.leaves())
    }

    fn locked_p2_puzzle_hash(&self) -> Bytes32 {
        auction_lock_p2_puzzle_hash(self.launcher_id)
    }

    fn locked_full_puzzle_hash(&self) -> Bytes32 {
        let p2_puzzle_hash = self.locked_p2_puzzle_hash();

        match self.asset {
            AuctionAsset::Xch => p2_puzzle_hash,
            AuctionAsset::Cat { asset_id } => CatInfo::new(asset_id, None, p2_puzzle_hash)
                .puzzle_hash()
                .into(),
            AuctionAsset::RevocableCat {
                asset_id,
                hidden_puzzle_hash,
            } => CatInfo::new(asset_id, Some(hidden_puzzle_hash), p2_puzzle_hash)
                .puzzle_hash()
                .into(),
        }
    }
}

impl SingletonInfo for AuctionInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            ReserveFinalizer2ndCurryArgs::curry_tree_hash(
                self.locked_full_puzzle_hash(),
                self.locked_p2_puzzle_hash(),
                RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
                self.launcher_id,
            ),
            self.merkle_tree().root(),
            self.state.tree_hash(),
        )
    }
}

pub trait AuctionLauncherExt {
    fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        asset: AuctionAsset,
        nft_coin_id: Bytes32,
    ) -> Result<(Conditions, Auction), DriverError>;
}

impl AuctionLauncherExt for Launcher {
    fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        asset: AuctionAsset,
        nft_coin_id: Bytes32,
    ) -> Result<(Conditions, Auction), DriverError> {
        let launcher_coin = self.coin();

        let info = AuctionInfo::new(
            launcher_coin.coin_id(),
            settings,
            asset,
            nft_coin_id,
            AuctionState::initial(settings.payments.payout_puzzle_hash),
        );

        let (conditions, coin) = self.spend(
            ctx,
            info.inner_puzzle_hash().into(),
            AuctionMemo { settings },
        )?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((conditions, Auction::new(coin, proof, info)))
    }
}

pub fn auction_lock_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
    mips_puzzle_hash(
        0,
        vec![],
        SingletonMember::new(launcher_id).curry_tree_hash(),
        true,
    )
    .into()
}

pub fn spend_auction_lock(
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    inner_puzzle_hash: TreeHash,
    conditions: Conditions,
) -> Result<Spend, DriverError> {
    let mut mips_spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

    let p2_puzzle_hash = auction_lock_p2_puzzle_hash(launcher_id);
    let puzzle = ctx.curry(SingletonMember::new(launcher_id))?;
    let solution = ctx.alloc(&SingletonMemberSolution::new(inner_puzzle_hash.into(), 1))?;

    mips_spend.members.insert(
        p2_puzzle_hash.into(),
        InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, solution)),
    );

    mips_spend.spend(ctx, p2_puzzle_hash.into())
}

pub trait AuctionExt: Sized {
    fn spend(
        self,
        ctx: &mut SpendContext,
        reserve_coin_id: Bytes32,
        action_spends: Vec<Spend>,
    ) -> Result<Self, DriverError>;
}

impl AuctionExt for Auction {
    fn spend(
        self,
        ctx: &mut SpendContext,
        reserve_coin_id: Bytes32,
        action_spends: Vec<Spend>,
    ) -> Result<Self, DriverError> {
        let merkle_tree = self.info.merkle_tree();
        let reserve_amount_from_state_program = node_from_bytes(
            ctx,
            &RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM,
        )?;

        let action_layer = ActionLayer::new(
            merkle_tree.root(),
            self.info.state,
            Finalizer::Reserve {
                reserve_full_puzzle_hash: self.info.locked_full_puzzle_hash(),
                reserve_inner_puzzle_hash: self.info.locked_p2_puzzle_hash(),
                reserve_amount_from_state_program,
                hint: self.info.launcher_id,
            },
        );

        let action_spend_hashes = action_spends
            .iter()
            .map(|spend| ctx.tree_hash(spend.puzzle).into())
            .collect::<Vec<_>>();

        let proofs = action_layer
            .get_proofs(&self.info.leaves(), &action_spend_hashes)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let finalizer_solution = ctx.alloc(&ReserveFinalizerSolution {
            reserve_parent_id: reserve_coin_id,
        })?;

        let mut state = self.info.state;

        for action_spend in &action_spends {
            let puzzle = Puzzle::parse(ctx, action_spend.puzzle);

            if puzzle.mod_hash() == BidActionArgs::<NodePtr>::mod_hash() {
                let solution = ctx.extract::<BidActionSolution>(action_spend.solution)?;

                state.winning_bid = solution.bid;
                state.reserve_amount = solution.bid.amount
                    + calculate_bps_payment(
                        solution.bid.amount,
                        self.info.settings.payments.buyers_premium.bps,
                    );
            } else if puzzle.mod_hash() == EndActionArgs::mod_hash() {
                state.reserve_amount = 0;
                break;
            }
        }

        let inner_spend = action_layer.construct_spend(
            ctx,
            ActionLayerSolution {
                proofs,
                action_spends,
                finalizer_solution,
            },
        )?;

        let coin_spend = SingletonLayer::new(self.info.launcher_id, inner_spend.puzzle)
            .construct_coin_spend(
                ctx,
                self.coin,
                SingletonSolution {
                    lineage_proof: self.proof,
                    amount: self.coin.amount,
                    inner_solution: inner_spend.solution,
                },
            )?;

        ctx.insert(coin_spend);

        Ok(self.child_with(
            AuctionInfo::new(
                self.info.launcher_id,
                self.info.settings,
                self.info.asset,
                self.info.nft_coin_id,
                state,
            ),
            self.coin.amount,
        ))
    }
}
