mod common;

use anyhow::Result;
use common::*;

const CAT_RESERVES: [ReserveKind; 2] = [ReserveKind::Cat, ReserveKind::RevocableCat];

#[test]
fn cat_auction_pays_everyone() -> Result<()> {
    for reserve in CAT_RESERVES {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        // Amounts where every fee rounds down
        let first = h.bid(150)?;
        assert_eq!(first.reserve_amount, 150 + 4 + 7);

        let outbid_reserve = h.auction.info.reserve;
        let second = h.bid(250)?;
        assert_eq!(second.reserve_amount, 250 + 7 + 12);
        h.assert_unspent(h.reserve_output(
            outbid_reserve,
            first.bid.puzzle_hash,
            first.reserve_amount,
        ));

        h.set_time(END_TIME)?;
        let final_reserve = h.auction.info.reserve;
        let nft = h.end()?;

        h.assert_settled(final_reserve, 250);
        assert_eq!(nft.info.p2_puzzle_hash, second.bid.puzzle_hash);
    }

    Ok(())
}

#[test]
fn cat_auction_with_batched_bids() -> Result<()> {
    for reserve in CAT_RESERVES {
        let mut h = Harness::new(Config {
            reserve,
            ..Config::default()
        })?;

        let reserve_before = h.auction.info.reserve;
        let bidders = h.bids(&[(100, false), (200, false), (300, false)])?;

        for outbid in &bidders[..2] {
            h.assert_unspent(h.reserve_output(
                reserve_before,
                outbid.bid.puzzle_hash,
                outbid.reserve_amount,
            ));
        }

        h.set_time(END_TIME)?;
        let final_reserve = h.auction.info.reserve;
        let nft = h.end()?;

        h.assert_settled(final_reserve, 300);
        assert_eq!(nft.info.p2_puzzle_hash, bidders[2].bid.puzzle_hash);
    }

    Ok(())
}

#[test]
fn cat_auction_without_fees() -> Result<()> {
    for reserve in CAT_RESERVES {
        let mut h = Harness::new(Config {
            reserve,
            buyers_premium_bps: 0,
            commission_bps: 0,
            royalty_bps: 0,
            ..Config::default()
        })?;

        h.bid(100)?;
        h.set_time(END_TIME)?;
        let final_reserve = h.auction.info.reserve;
        let _ = h.end()?;

        h.assert_settled(final_reserve, 100);
    }

    Ok(())
}
