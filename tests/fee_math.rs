use std::collections::HashMap;

use auction::*;
use chia_wallet_sdk::{
    chia::puzzle_types::{EveProof, Proof},
    clvm_traits::clvm_list,
    prelude::*,
    puzzles::SETTLEMENT_PAYMENT_HASH,
};
use proptest::prelude::*;

const PREMIUM_PUZZLE_HASH: Bytes32 = Bytes32::new([1; 32]);
const COMMISSION_PUZZLE_HASH: Bytes32 = Bytes32::new([2; 32]);
const PAYOUT_PUZZLE_HASH: Bytes32 = Bytes32::new([3; 32]);
const BIDDER_PUZZLE_HASH: Bytes32 = Bytes32::new([4; 32]);

/// An auction that is never launched, since only its puzzles are run.
fn auction(premium_bps: u64, commission_bps: u64, royalty_bps: u16) -> Auction {
    let launcher_id = Bytes32::new([5; 32]);
    let settings = AuctionSettings {
        bid_verifier: BidVerifier::Flat {
            minimum_bid: 1,
            bid_increment: 1,
        },
        timings: Timings::new(1_000, 0),
        payments: Payments {
            buyers_premium: BpsPayment::new(premium_bps, PREMIUM_PUZZLE_HASH),
            commission: BpsPayment::new(commission_bps, COMMISSION_PUZZLE_HASH),
            payout_puzzle_hash: PAYOUT_PUZZLE_HASH,
        },
    };
    let reserve = Coin::new(
        Bytes32::new([6; 32]),
        auction_lock_p2_puzzle_hash(launcher_id),
        0,
    );
    let info = AuctionInfo::new(
        launcher_id,
        settings,
        Bytes32::new([7; 32]),
        RoyaltyInfo::new(Bytes32::new([8; 32]), Bytes32::new([9; 32]), royalty_bps),
        AuctionState::initial(PAYOUT_PUZZLE_HASH),
        AuctionReserve::Xch(reserve),
    );
    let proof = Proof::Eve(EveProof {
        parent_parent_coin_info: Bytes32::default(),
        parent_amount: 1,
    });
    Auction::new(Coin::new(launcher_id, Bytes32::default(), 1), proof, info)
}

proptest! {
    #[test]
    fn driver_fee_math_matches_puzzles(
        // Small enough that the reserve amount always fits in a u64
        amount in 1..=u64::MAX / 3,
        premium_bps in 0..=MAX_BPS,
        commission_bps in 0..=MAX_BPS,
        royalty_bps in 0..=10_000u16,
    ) {
        let mut ctx = SpendContext::new();
        let mut auction = auction(premium_bps, commission_bps, royalty_bps);

        let bid = Bid::new(amount, BIDDER_PUZZLE_HASH);
        let bid_action = auction.spend_bid_action(&mut ctx, bid, false).unwrap();
        let child = auction.child_state(&mut ctx, &[bid_action]).unwrap();

        let reserve_amount = auction.info.reserve_amount_for_bid(amount).unwrap();
        prop_assert_eq!(child.state.reserve_amount, reserve_amount);

        auction.info.state = child.state;
        let end_puzzle = auction.info.end_action(&mut ctx).unwrap();
        let end_solution = ctx.alloc(&clvm_list!(1u64)).unwrap();
        let child = auction
            .child_state(&mut ctx, &[Spend::new(end_puzzle, end_solution)])
            .unwrap();

        let mut payments = HashMap::new();
        for condition in child.reserve_conditions {
            let create_coin = ctx.extract::<CreateCoin<NodePtr>>(condition).unwrap();
            prop_assert!(payments.insert(create_coin.puzzle_hash, create_coin.amount).is_none());
        }

        let premium = calculate_bps_payment(amount, premium_bps).unwrap();
        let commission = calculate_bps_payment(amount, commission_bps).unwrap();
        let royalty = calculate_bps_payment(amount, royalty_bps.into()).unwrap();

        let expected = [
            (PREMIUM_PUZZLE_HASH, premium),
            (COMMISSION_PUZZLE_HASH, commission),
            (PAYOUT_PUZZLE_HASH, amount - commission),
            (SETTLEMENT_PAYMENT_HASH.into(), royalty),
        ];

        for (puzzle_hash, expected) in expected {
            prop_assert_eq!(payments.remove(&puzzle_hash).unwrap_or(0), expected);
        }

        // Only the empty reserve for the ended auction is left
        prop_assert_eq!(payments.remove(&auction.info.reserve.p2_puzzle_hash()), Some(0));
        prop_assert!(payments.is_empty());

        prop_assert_eq!(child.royalty_amount, (royalty > 0).then_some(royalty));
        prop_assert_eq!(premium + royalty + amount, reserve_amount);
    }
}
