use chia_wallet_sdk::{
    driver::{InnerPuzzleSpend, MipsSpend, mips_puzzle_hash},
    prelude::*,
    types::puzzles::{SingletonMember, SingletonMemberSolution},
};

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
