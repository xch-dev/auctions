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
    AuctionInfo, AuctionReserve, AuctionState, Bid, BidActionArgs, BidActionSolution,
    EndActionArgs, calculate_bps_payment, spend_auction_lock,
};

pub type Auction = Singleton<AuctionInfo>;

pub trait AuctionExt: Sized {
    fn spend_bid_action(
        &self,
        ctx: &mut SpendContext,
        bid: Bid,
        grace: bool,
    ) -> Result<Spend, DriverError>;

    fn spend_end_action(&self, ctx: &mut SpendContext) -> Result<Spend, DriverError>;

    fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<(Conditions, AuctionState), DriverError>;

    fn spend(
        self,
        ctx: &mut SpendContext,
        action_spends: Vec<Spend>,
        other_cat_spends: Vec<CatSpend>,
    ) -> Result<Self, DriverError>;
}

impl AuctionExt for Auction {
    fn spend_bid_action(
        &self,
        ctx: &mut SpendContext,
        bid: Bid,
        grace: bool,
    ) -> Result<Spend, DriverError> {
        let puzzle = self.info.bid_action(ctx)?;
        let solution = ctx.alloc(&BidActionSolution::new(bid, grace))?;
        Ok(Spend::new(puzzle, solution))
    }

    fn spend_end_action(&self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let puzzle = self.info.end_action(ctx)?;
        Ok(Spend::new(puzzle, NodePtr::NIL))
    }

    fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<(Conditions, AuctionState), DriverError> {
        let mut reserve_conditions = Conditions::new();
        let mut state = self.info.state;

        for action_spend in action_spends {
            let puzzle = Puzzle::parse(ctx, action_spend.puzzle);

            if puzzle.mod_hash() == BidActionArgs::<NodePtr>::mod_hash() {
                let solution = ctx.extract::<BidActionSolution>(action_spend.solution)?;

                if state.winning_bid.amount > 0 {
                    let hint = ctx.hint(state.winning_bid.puzzle_hash)?;
                    reserve_conditions.push(CreateCoin::new(
                        state.winning_bid.puzzle_hash,
                        state.reserve_amount,
                        hint,
                    ));
                }

                state.winning_bid = solution.bid;
                state.reserve_amount = solution.bid.amount
                    + calculate_bps_payment(
                        solution.bid.amount,
                        self.info.settings.payments.buyers_premium.bps,
                    );
            } else if puzzle.mod_hash() == EndActionArgs::mod_hash() {
                let buyers_premium = self.info.settings.payments.buyers_premium;
                let commission = self.info.settings.payments.commission;
                let payout_puzzle_hash = self.info.settings.payments.payout_puzzle_hash;

                let buyers_premium_amount =
                    calculate_bps_payment(state.winning_bid.amount, buyers_premium.bps);

                let commission_amount =
                    calculate_bps_payment(state.winning_bid.amount, commission.bps);

                let payout_amount = state.winning_bid.amount - commission_amount;

                if buyers_premium_amount > 0 {
                    reserve_conditions.push(CreateCoin::new(
                        buyers_premium.puzzle_hash,
                        buyers_premium_amount,
                        ctx.hint(buyers_premium.puzzle_hash)?,
                    ));
                } else {
                    reserve_conditions.push(Remark::new(NodePtr::NIL));
                }

                if commission_amount > 0 {
                    reserve_conditions.push(CreateCoin::new(
                        commission.puzzle_hash,
                        commission_amount,
                        ctx.hint(commission.puzzle_hash)?,
                    ));
                } else {
                    reserve_conditions.push(Remark::new(NodePtr::NIL));
                }

                if payout_amount > 0 {
                    reserve_conditions.push(CreateCoin::new(
                        payout_puzzle_hash,
                        payout_amount,
                        ctx.hint(payout_puzzle_hash)?,
                    ));
                } else {
                    reserve_conditions.push(Remark::new(NodePtr::NIL));
                }

                state.reserve_amount = 0;
            }
        }

        let mut reserve_conditions = reserve_conditions.into_iter().collect::<Vec<_>>();

        reserve_conditions.reverse();

        reserve_conditions.insert(
            0,
            Condition::CreateCoin(CreateCoin::new(
                self.info.reserve.p2_puzzle_hash(),
                state.reserve_amount,
                ctx.hint(self.info.reserve.p2_puzzle_hash())?,
            )),
        );

        Ok((reserve_conditions.into(), state))
    }

    fn spend(
        self,
        ctx: &mut SpendContext,
        action_spends: Vec<Spend>,
        mut other_cat_spends: Vec<CatSpend>,
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
                reserve_full_puzzle_hash: self.info.reserve.coin().puzzle_hash,
                reserve_inner_puzzle_hash: self.info.reserve.p2_puzzle_hash(),
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
            reserve_parent_id: self.info.reserve.coin().parent_coin_info,
        })?;

        let (reserve_conditions, state) = self.child_state(ctx, &action_spends)?;

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

        let reserve_spend = spend_auction_lock(
            ctx,
            self.info.launcher_id,
            self.info.inner_puzzle_hash(),
            reserve_conditions,
        )?;

        let new_reserve = match self.info.reserve {
            AuctionReserve::Xch(coin) => {
                ctx.spend(coin, reserve_spend)?;
                AuctionReserve::Xch(Coin::new(
                    coin.coin_id(),
                    coin.puzzle_hash,
                    state.reserve_amount,
                ))
            }
            AuctionReserve::Cat(cat) => {
                let cat_spend = CatSpend::new(cat, reserve_spend);
                other_cat_spends.push(cat_spend);
                Cat::spend_all(ctx, &other_cat_spends)?;
                AuctionReserve::Cat(cat.child(cat.info.p2_puzzle_hash, state.reserve_amount))
            }
        };

        let child = self.child_with(
            AuctionInfo::new(
                self.info.launcher_id,
                self.info.settings,
                self.info.nft_coin_id,
                state,
                new_reserve,
            ),
            self.coin.amount,
        );

        Ok(child)
    }
}
