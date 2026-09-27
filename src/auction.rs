use chia_wallet_sdk::{
    chia::puzzle_types::{
        offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
        singleton::SingletonSolution,
    },
    clvm_traits::{clvm_list, clvm_quote},
    clvmr::serde::node_from_bytes,
    driver::{ActionLayer, ActionLayerSolution, Finalizer, Layer, SingletonLayer},
    prelude::*,
    puzzles::SETTLEMENT_PAYMENT_HASH,
    types::puzzles::{
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM, ReserveFinalizerSolution,
    },
};

use crate::{
    AuctionInfo, AuctionReserve, AuctionState, Bid, BidActionSolution, NftUnlockerSolution,
    spend_auction_lock,
};

pub type Auction = Singleton<AuctionInfo>;

const RESERVE_CONDITION_OPCODE: i64 = -42;

#[derive(Debug, Clone)]
pub struct AuctionChildState {
    pub state: AuctionState,
    /// The conditions output by the reserve coin, including the creation of the new reserve.
    pub reserve_conditions: Vec<NodePtr>,
    /// The amount of the ephemeral settlement coin that pays the NFT royalty, if one is created.
    pub royalty_amount: Option<u64>,
}

pub trait AuctionExt: Sized {
    fn spend_bid_action(
        &self,
        ctx: &mut SpendContext,
        bid: Bid,
        grace: bool,
    ) -> Result<Spend, DriverError>;

    /// Ends the auction. `nft` is the locked NFT, which must be unlocked with
    /// [`unlock_nft`](AuctionExt::unlock_nft) in the same spend bundle.
    fn spend_end_action(&self, ctx: &mut SpendContext, nft: &Nft) -> Result<Spend, DriverError>;

    /// Spends the locked NFT to the winning bidder. This must be included in the same spend bundle as
    /// the end action, and `self` must be the auction before it's ended.
    fn unlock_nft(&self, ctx: &mut SpendContext, nft: &Nft) -> Result<Nft, DriverError>;

    /// Runs the action puzzles to determine the next state and the reserve coin's conditions, exactly
    /// as the action layer and reserve finalizer will on chain.
    fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<AuctionChildState, DriverError>;

    fn spend(
        self,
        ctx: &mut SpendContext,
        action_spends: Vec<Spend>,
        other_cat_spends: Vec<CatSpend>,
    ) -> Result<Self, DriverError>;

    /// Parses the child of this auction, given the solution this auction's coin was spent with on
    /// the blockchain.
    fn parse_child(&self, ctx: &mut SpendContext, solution: NodePtr) -> Result<Self, DriverError>;
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

    fn spend_end_action(&self, ctx: &mut SpendContext, nft: &Nft) -> Result<Spend, DriverError> {
        let puzzle = self.info.end_action(ctx)?;
        // The end action passes its solution to the unlocker after the winning bid
        let solution = ctx.alloc(&clvm_list!(nft.coin.amount))?;
        Ok(Spend::new(puzzle, solution))
    }

    fn unlock_nft(&self, ctx: &mut SpendContext, nft: &Nft) -> Result<Nft, DriverError> {
        let unlocker = self.info.nft_unlocker(ctx)?;
        let unlocker_solution = ctx.alloc(&NftUnlockerSolution::new(
            self.info.state.winning_bid,
            nft.coin.amount,
        ))?;
        let conditions = ctx.run(unlocker, unlocker_solution)?;
        let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;

        let spend = spend_auction_lock(
            ctx,
            self.info.launcher_id,
            self.info.inner_puzzle_hash(),
            Spend::new(delegated_puzzle, NodePtr::NIL),
        )?;

        nft.spend(ctx, spend)
    }

    fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<AuctionChildState, DriverError> {
        let mut state = self.info.state;
        let mut truth = ctx.alloc(&(NodePtr::NIL, state))?;
        let mut reserve_conditions = Vec::new();
        let mut royalty_amount = None;

        for action_spend in action_spends {
            let solution = ctx.alloc(&(truth, action_spend.solution))?;
            let output = ctx.run(action_spend.puzzle, solution)?;
            let (new_truth, conditions) = ctx.extract::<(NodePtr, Vec<NodePtr>)>(output)?;
            let (_, new_state) = ctx.extract::<(NodePtr, AuctionState)>(new_truth)?;

            // The reserve finalizer collects each action's reserve conditions in reverse order
            let mut action_reserve_conditions = Vec::new();

            for condition in conditions.into_iter().rev() {
                let (opcode, condition) = ctx.extract::<(i64, NodePtr)>(condition)?;

                if opcode == RESERVE_CONDITION_OPCODE {
                    action_reserve_conditions.push(condition);
                }
            }

            // Only the end action pays the royalty. A bid refund can also be sent to the settlement
            // puzzle hash, if that's the puzzle hash the bidder chose.
            if new_state.ended && !state.ended {
                for &condition in &action_reserve_conditions {
                    if let Ok(create_coin) = ctx.extract::<CreateCoin<NodePtr>>(condition)
                        && create_coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
                    {
                        royalty_amount = Some(create_coin.amount);
                    }
                }
            }

            reserve_conditions.extend(action_reserve_conditions);
            truth = new_truth;
            state = new_state;
        }

        let p2_puzzle_hash = self.info.reserve.p2_puzzle_hash();
        let hint = ctx.hint(p2_puzzle_hash)?;
        let new_reserve =
            ctx.alloc(&CreateCoin::new(p2_puzzle_hash, state.reserve_amount, hint))?;
        reserve_conditions.insert(0, new_reserve);

        Ok(AuctionChildState {
            state,
            reserve_conditions,
            royalty_amount,
        })
    }

    fn spend(
        self,
        ctx: &mut SpendContext,
        action_spends: Vec<Spend>,
        mut other_cat_spends: Vec<CatSpend>,
    ) -> Result<Self, DriverError> {
        let merkle_tree = self.info.merkle_tree();

        let AuctionChildState {
            state,
            reserve_conditions,
            royalty_amount,
        } = self.child_state(ctx, &action_spends)?;

        let royalty_payment = if let Some(amount) = royalty_amount {
            let royalty_puzzle_hash = self.info.nft_royalty.puzzle_hash;
            Some((
                amount,
                NotarizedPayment::new(
                    self.info.nft_royalty.launcher_id,
                    vec![Payment::new(
                        royalty_puzzle_hash,
                        amount,
                        ctx.hint(royalty_puzzle_hash)?,
                    )],
                ),
            ))
        } else {
            None
        };

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

        let delegated_puzzle = ctx.alloc(&clvm_quote!(reserve_conditions))?;

        let reserve_spend = spend_auction_lock(
            ctx,
            self.info.launcher_id,
            self.info.inner_puzzle_hash(),
            Spend::new(delegated_puzzle, NodePtr::NIL),
        )?;

        match self.info.reserve {
            AuctionReserve::Xch(coin) => {
                ctx.spend(coin, reserve_spend)?;

                if let Some((amount, royalty_payment)) = royalty_payment {
                    let settlement_coin =
                        Coin::new(coin.coin_id(), SETTLEMENT_PAYMENT_HASH.into(), amount);
                    let settlement_spend = SettlementLayer.construct_coin_spend(
                        ctx,
                        settlement_coin,
                        SettlementPaymentsSolution::new(vec![royalty_payment]),
                    )?;
                    ctx.insert(settlement_spend);
                }
            }
            AuctionReserve::Cat(cat) => {
                if let Some((amount, royalty_payment)) = royalty_payment {
                    let settlement_spend = SettlementLayer.construct_spend(
                        ctx,
                        SettlementPaymentsSolution::new(vec![royalty_payment]),
                    )?;
                    other_cat_spends.push(CatSpend::new(
                        cat.child(SETTLEMENT_PAYMENT_HASH.into(), amount),
                        settlement_spend,
                    ));
                }

                other_cat_spends.push(CatSpend::new(cat, reserve_spend));
                Cat::spend_all(ctx, &other_cat_spends)?;
            }
        }

        Ok(child_auction(&self, self.info.reserve, state))
    }

    fn parse_child(&self, ctx: &mut SpendContext, solution: NodePtr) -> Result<Self, DriverError> {
        let singleton_solution = SingletonSolution::<NodePtr>::from_clvm(ctx, solution)?;
        let action_layer_solution = ActionLayer::<AuctionState, NodePtr>::parse_solution(
            ctx,
            singleton_solution.inner_solution,
        )?;
        let finalizer_solution =
            ctx.extract::<ReserveFinalizerSolution>(action_layer_solution.finalizer_solution)?;
        // Whoever spent the auction chose which coin to spend as the reserve, so it may not be the
        // reserve we were tracking
        let spent_reserve = self
            .info
            .reserve
            .with_parent_coin_info(finalizer_solution.reserve_parent_id);
        let child = self.child_state(ctx, &action_layer_solution.action_spends)?;
        Ok(child_auction(self, spent_reserve, child.state))
    }
}

fn child_auction(auction: &Auction, spent_reserve: AuctionReserve, state: AuctionState) -> Auction {
    auction.child_with(
        AuctionInfo {
            state,
            reserve: spent_reserve.child(state.reserve_amount),
            ..auction.info
        },
        auction.coin.amount,
    )
}
