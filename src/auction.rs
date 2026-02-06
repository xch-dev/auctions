use chia_wallet_sdk::{
    chia::puzzle_types::singleton::SingletonSolution,
    clvmr::serde::node_from_bytes,
    driver::{ActionLayer, ActionLayerSolution, Finalizer, SingletonLayer},
    prelude::*,
    types::puzzles::{
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM, ReserveFinalizerSolution,
    },
};

use crate::{
    AuctionInfo, AuctionState, BidActionArgs, BidActionSolution, EndActionArgs,
    calculate_bps_payment,
};

pub type Auction = Singleton<AuctionInfo>;

pub trait AuctionExt: Sized {
    fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<AuctionState, DriverError>;

    fn spend(
        self,
        ctx: &mut SpendContext,
        reserve_coin_id: Bytes32,
        action_spends: Vec<Spend>,
    ) -> Result<Self, DriverError>;
}

impl AuctionExt for Auction {
    fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<AuctionState, DriverError> {
        let mut state = self.info.state;

        for action_spend in action_spends {
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

        Ok(state)
    }

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
            .get_proofs(&self.info.merkle_leaves(), &action_spend_hashes)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let finalizer_solution = ctx.alloc(&ReserveFinalizerSolution {
            reserve_parent_id: reserve_coin_id,
        })?;

        let state = self.child_state(ctx, &action_spends)?;

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
