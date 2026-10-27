mod common;

use anyhow::{Result, anyhow};
use auction::*;
use chia_wallet_sdk::prelude::*;
use common::*;

#[test]
fn cancel_returns_nft_to_seller() -> Result<()> {
    for reserve in [
        ReserveKind::Xch,
        ReserveKind::Cat,
        ReserveKind::RevocableCat,
    ] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        let reserve_before = h.auction.info.reserve;
        let nft = h.cancel()?;

        h.assert_unspent(nft.coin);
        assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
        assert_eq!(nft.info.current_owner, None);

        assert!(h.auction.info.state.ended);
        assert_eq!(h.auction.info.state.reserve_amount, 0);
        assert_eq!(
            h.auction.info.state.winning_bid,
            Bid::new(0, h.seller.puzzle_hash)
        );

        // Nothing is paid out, and the reserve is recreated empty
        h.assert_missing(h.reserve_output(reserve_before, h.seller.puzzle_hash, 0));
        let child_reserve = h.auction.info.reserve.coin();
        assert_eq!(
            child_reserve.parent_coin_info,
            reserve_before.coin().coin_id()
        );
        assert_eq!(child_reserve.amount, 0);
        h.assert_unspent(child_reserve);

        // A cancelled auction can't be bid on, ended, or cancelled again
        assert_fails_with(h.bid(100), "raise");
        h.set_time(END_TIME)?;
        assert_fails_with(h.end(), "raise");
        assert_fails_with(h.cancel(), "raise");
    }

    Ok(())
}

#[test]
fn cancel_requires_the_sellers_message() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    assert_fails_with(h.cancel_authorized_by(None), "MessageNotSentOrReceived");

    // A message from any puzzle other than the payout puzzle doesn't authorize the cancel
    let stranger = h.sim.bls(0);
    assert_fails_with(
        h.cancel_authorized_by(Some(stranger)),
        "MessageNotSentOrReceived",
    );

    // The auction is untouched, so the seller can still cancel it
    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn cannot_cancel_after_a_bid() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let winner = h.bid(100)?;

    assert_fails_with(h.cancel(), "raise");

    // The bid is still live and wins the auction
    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn cannot_cancel_after_the_auction_ended() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.set_time(END_TIME)?;
    let nft = h.end()?;

    assert_fails_with(h.cancel(), "raise");
    h.assert_unspent(nft.coin);

    Ok(())
}

#[test]
fn cancel_has_no_timing_restrictions() -> Result<()> {
    // Well before the end time, and with a grace period that would otherwise delay the end
    let mut h = Harness::new(Config {
        timings: Timings::new(END_TIME, 500),
        ..Config::default()
    })?;
    assert!(h.sim.next_timestamp() < END_TIME);

    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn parse_child_follows_a_cancel() -> Result<()> {
    for reserve in [
        ReserveKind::Xch,
        ReserveKind::Cat,
        ReserveKind::RevocableCat,
    ] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        let before = h.auction;
        let _ = h.cancel()?;

        // The harness already compares the parsed child to the spent one, but check it explicitly
        let coin_spend = h
            .sim
            .coin_spend(before.coin.coin_id())
            .ok_or(anyhow!("missing auction spend"))?;
        let solution = coin_spend.solution.to_clvm(&mut h.ctx)?;
        let parsed = before.parse_child(&mut h.ctx, solution)?;

        assert_eq!(parsed, h.auction);
        assert!(parsed.info.state.ended);
        assert_eq!(parsed.info.state.reserve_amount, 0);
        h.assert_unspent(parsed.coin);
        h.assert_unspent(parsed.info.reserve.coin());
    }

    Ok(())
}

/// The error for a spend bundle where the auction's puzzle raises during consensus validation.
const PUZZLE_RAISED: &str = "GeneratorRuntimeError";

/// Authorizes a cancel from a new seller coin, and returns the cancel action spend along with the
/// key that must sign the authorization.
fn authorized_cancel(h: &mut Harness) -> Result<(Spend, SecretKey)> {
    let authorizer = h.seller_coin();
    let conditions = h.auction.cancel_authorization(&mut h.ctx)?;
    StandardLayer::new(authorizer.pk).spend(&mut h.ctx, authorizer.coin, conditions)?;
    let cancel = h.auction.spend_cancel_action(&mut h.ctx, &h.nft)?;
    Ok((cancel, authorizer.sk))
}

#[test]
fn cancel_rejects_other_authorizations() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let auction_coin = h.auction.coin;

    let wrong_authorizations: [fn(&mut Harness, Coin) -> Result<Conditions>; 4] = [
        // Sent to the auction's puzzle hash rather than its coin
        |h, auction_coin| {
            let receiver = h.ctx.alloc(&auction_coin.puzzle_hash)?;
            Ok(Conditions::new().send_message(
                0b010_010,
                CANCEL_MESSAGE.to_vec().into(),
                vec![receiver],
            ))
        },
        // Sent to a different coin
        |h, _| {
            let receiver = h.ctx.alloc(&h.nft.coin.coin_id())?;
            Ok(Conditions::new().send_message(
                CANCEL_MESSAGE_MODE,
                CANCEL_MESSAGE.to_vec().into(),
                vec![receiver],
            ))
        },
        // Committing to the sender's coin rather than its puzzle
        |h, auction_coin| {
            let receiver = h.ctx.alloc(&auction_coin.coin_id())?;
            Ok(Conditions::new().send_message(
                0b111_111,
                CANCEL_MESSAGE.to_vec().into(),
                vec![receiver],
            ))
        },
        // The wrong message
        |h, auction_coin| {
            let receiver = h.ctx.alloc(&auction_coin.coin_id())?;
            Ok(Conditions::new().send_message(
                CANCEL_MESSAGE_MODE,
                b"cancel".to_vec().into(),
                vec![receiver],
            ))
        },
    ];

    for wrong_authorization in wrong_authorizations {
        let authorizer = h.seller_coin();
        let conditions = wrong_authorization(&mut h, auction_coin)?;
        assert_fails_with(
            h.cancel_with_authorization(Some((authorizer, conditions))),
            "MessageNotSentOrReceived",
        );
    }

    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn unchecked_cancel_without_bids_succeeds() -> Result<()> {
    // Makes sure that the unchecked spends below are only rejected because of the cancel action
    let mut h = Harness::new(Config::default())?;
    let (cancel, key) = authorized_cancel(&mut h)?;
    let nft = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;
    h.spend_unchecked(vec![cancel], 0, &[key])?;

    h.assert_unspent(nft.coin);
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
    assert!(h.auction.info.state.ended);

    Ok(())
}

#[test]
fn cancel_after_a_bid_is_rejected_on_chain() -> Result<()> {
    // Without a royalty, the NFT's transfer doesn't assert a royalty payment, so nothing but the
    // cancel action's own check stops a cancel from sending the NFT to the bidder and burning the
    // reserve as fees
    for royalty_bps in [0, 500] {
        let mut h = Harness::new(Config {
            royalty_bps,
            ..Config::default()
        })?;
        let winner = h.bid(100)?;

        let (cancel, key) = authorized_cancel(&mut h)?;
        let _ = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;
        let result = h.spend_unchecked(vec![cancel], 0, &[key]);
        assert_fails_with(result, PUZZLE_RAISED);

        assert_eq!(h.auction.info.state.winning_bid, winner.bid);
        h.assert_unspent(h.auction.info.reserve.coin());
    }

    Ok(())
}

#[test]
fn cancel_after_the_end_is_rejected_on_chain() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;

    // The NFT has already been unlocked, so only the cancel is attempted
    let (cancel, key) = authorized_cancel(&mut h)?;
    let result = h.spend_unchecked(vec![cancel], 0, &[key]);
    assert_fails_with(result, PUZZLE_RAISED);

    Ok(())
}

#[test]
fn cancel_cannot_be_combined_with_other_actions() -> Result<()> {
    #[derive(Debug, Clone, Copy)]
    enum Other {
        Bid,
        End,
    }

    for (other, cancel_first) in [
        (Other::Bid, false),
        (Other::Bid, true),
        (Other::End, false),
        (Other::End, true),
    ] {
        let mut h = Harness::new(Config::default())?;

        // The bid is never funded, since the spend is rejected before its announcement matters
        let other_spend = match other {
            Other::Bid => {
                let bid = Bid::new(100, Bytes32::new([5; 32]));
                h.auction.spend_bid_action(&mut h.ctx, bid, false)?
            }
            Other::End => {
                h.set_time(END_TIME)?;
                h.auction.spend_end_action(&mut h.ctx, &h.nft)?
            }
        };

        let (cancel, key) = authorized_cancel(&mut h)?;
        let _ = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;

        let actions = if cancel_first {
            vec![cancel, other_spend]
        } else {
            vec![other_spend, cancel]
        };
        let result = h.spend_unchecked(actions, 0, &[key]);
        assert_fails_with(result, PUZZLE_RAISED);
    }

    Ok(())
}

#[test]
fn two_cancels_in_one_spend_are_rejected() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    // Each cancel has its own authorization, so only the second cancel's check can reject it
    let (first, first_key) = authorized_cancel(&mut h)?;
    let (second, second_key) = authorized_cancel(&mut h)?;
    let _ = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;
    let result = h.spend_unchecked(vec![first, second], 0, &[first_key, second_key]);
    assert_fails_with(result, PUZZLE_RAISED);

    Ok(())
}

#[test]
fn cancel_can_spend_a_stand_in_reserve() -> Result<()> {
    // If the launcher's reserve is missing, the seller can create an empty coin at the lock address
    // and spend it as the reserve instead. For CAT reserves, this needs a coin of the same CAT.
    for reserve in [
        ReserveKind::Xch,
        ReserveKind::Cat,
        ReserveKind::RevocableCat,
    ] {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        let tracked = h.auction.info.reserve;
        let stand_in = h.decoy_reserve()?;
        h.auction.info.reserve = stand_in;

        // The harness checks that parse_child follows the stand-in reserve
        let nft = h.cancel()?;
        assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

        h.assert_unspent(tracked.coin());
        let child_reserve = h.auction.info.reserve.coin();
        assert_eq!(child_reserve.parent_coin_info, stand_in.coin().coin_id());
        h.assert_unspent(child_reserve);
    }

    Ok(())
}
