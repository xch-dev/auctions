#![allow(dead_code)]

use std::fmt::Debug;

use anyhow::{Result, anyhow};
use auction::*;
use chia_wallet_sdk::{
    chia::consensus::{
        consensus_constants::TEST_CONSTANTS,
        spendbundle_conditions::get_conditions_from_spendbundle,
    },
    driver::IntermediateLauncher,
    prelude::*,
    puzzles::SETTLEMENT_PAYMENT_HASH,
    test::BlsPairWithCoin,
    types::announcement_id,
};

pub const PREMIUM_PUZZLE_HASH: Bytes32 = Bytes32::new([1; 32]);
pub const COMMISSION_PUZZLE_HASH: Bytes32 = Bytes32::new([2; 32]);
pub const ROYALTY_PUZZLE_HASH: Bytes32 = Bytes32::new([3; 32]);
pub const HIDDEN_PUZZLE_HASH: Bytes32 = Bytes32::new([4; 32]);

pub const END_TIME: u64 = 1_000;

const CAT_SUPPLY: u64 = 1_000_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveKind {
    Xch,
    Cat,
    RevocableCat,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub bid_verifier: BidVerifier,
    pub timings: Timings,
    pub buyers_premium_bps: u64,
    pub commission_bps: u64,
    pub royalty_bps: u16,
    pub reserve: ReserveKind,
    pub did_owner: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bid_verifier: BidVerifier::Flat {
                minimum_bid: 100,
                bid_increment: 100,
            },
            timings: Timings::new(END_TIME, 0),
            buyers_premium_bps: 300,
            commission_bps: 100,
            royalty_bps: 500,
            reserve: ReserveKind::Xch,
            did_owner: false,
        }
    }
}

impl Config {
    pub fn settings(&self, payout_puzzle_hash: Bytes32) -> AuctionSettings {
        AuctionSettings {
            bid_verifier: self.bid_verifier,
            timings: self.timings,
            payments: Payments {
                buyers_premium: BpsPayment::new(self.buyers_premium_bps, PREMIUM_PUZZLE_HASH),
                commission: BpsPayment::new(self.commission_bps, COMMISSION_PUZZLE_HASH),
                payout_puzzle_hash,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bidder {
    pub bid: Bid,
    pub reserve_amount: u64,
}

/// An auction for a freshly minted NFT, launched and confirmed in a simulator.
pub struct Harness {
    pub sim: Simulator,
    pub ctx: SpendContext,
    pub config: Config,
    pub seller: BlsPairWithCoin,
    pub auction: Auction,
    pub nft: Nft,
    bank: Option<(BlsPairWithCoin, Cat)>,
}

impl Harness {
    pub fn new(config: Config) -> Result<Self> {
        Self::with_settings(config, |config, seller, _| config.settings(seller))
    }

    /// Like [`Harness::new`], but `settings` is called with the config, the seller's puzzle hash, and
    /// the auction lock puzzle hash to build the auction settings.
    pub fn with_settings(
        config: Config,
        settings: impl FnOnce(&Config, Bytes32, Bytes32) -> AuctionSettings,
    ) -> Result<Self> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let seller = sim.bls(CAT_SUPPLY + 10);
        let seller_p2 = StandardLayer::new(seller.pk);
        let mut funding_coin = seller.coin;

        let did = if config.did_owner {
            let (create_did, did) =
                Launcher::new(funding_coin.coin_id(), 1).create_simple_did(&mut ctx, &seller_p2)?;
            let change = funding_coin.amount - 1;
            seller_p2.spend(
                &mut ctx,
                funding_coin,
                create_did.create_coin(seller.puzzle_hash, change, Memos::None),
            )?;
            sim.spend_coins(ctx.take(), std::slice::from_ref(&seller.sk))?;
            funding_coin = Coin::new(funding_coin.coin_id(), seller.puzzle_hash, change);
            Some(did)
        } else {
            None
        };

        let mut mint = NftMint::new(
            HashedPtr::NIL,
            seller.puzzle_hash,
            config.royalty_bps,
            did.map(|did| {
                TransferNft::new(
                    Some(did.info.launcher_id),
                    Vec::new(),
                    Some(did.info.inner_puzzle_hash().into()),
                )
            }),
        );
        mint.royalty_puzzle_hash = ROYALTY_PUZZLE_HASH;

        let mut conditions = Conditions::new();

        let nft = if let Some(did) = did {
            let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
                .create(&mut ctx)?
                .mint_nft(&mut ctx, &mint)?;
            let _ = did.update(&mut ctx, &seller_p2, mint_nft)?;
            nft
        } else {
            let (mint_nft, nft) = Launcher::new(funding_coin.coin_id(), 0)
                .with_singleton_amount(1)
                .mint_nft(&mut ctx, &mint)?;
            conditions = conditions.extend(mint_nft);
            nft
        };

        let launcher = Launcher::new(funding_coin.coin_id(), 1);
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(launcher.coin().coin_id());

        let locked_nft = nft.spend_with(
            &mut ctx,
            &seller_p2,
            Conditions::new().create_coin(lock_puzzle_hash, nft.coin.amount, Memos::None),
        )?;

        let mut change = funding_coin.amount - 2;
        let mut bank = None;

        let reserve = match config.reserve {
            ReserveKind::Xch => {
                conditions = conditions.create_coin(lock_puzzle_hash, 0, Memos::None);
                AuctionReserve::Xch(Coin::new(funding_coin.coin_id(), lock_puzzle_hash, 0))
            }
            ReserveKind::Cat | ReserveKind::RevocableCat => {
                let bank_key = sim.bls(0);
                let hidden_puzzle_hash =
                    (config.reserve == ReserveKind::RevocableCat).then_some(HIDDEN_PUZZLE_HASH);

                let (issue, cats) = Cat::single_issuance(
                    &mut ctx,
                    funding_coin.coin_id(),
                    hidden_puzzle_hash,
                    CAT_SUPPLY,
                    Conditions::new()
                        .create_coin(bank_key.puzzle_hash, CAT_SUPPLY, Memos::None)
                        .create_coin(lock_puzzle_hash, 0, Memos::None),
                )?;
                conditions = conditions.extend(issue);
                change -= CAT_SUPPLY;

                let find = |puzzle_hash| {
                    cats.iter()
                        .find(|cat| cat.info.p2_puzzle_hash == puzzle_hash)
                        .copied()
                        .ok_or(anyhow!("missing CAT output"))
                };

                let bank_cat = find(bank_key.puzzle_hash)?;
                bank = Some((bank_key, bank_cat));
                AuctionReserve::Cat(find(lock_puzzle_hash)?)
            }
        };

        let (launch, auction) = launcher.launch_auction(
            &mut ctx,
            settings(&config, seller.puzzle_hash, lock_puzzle_hash),
            reserve,
            &locked_nft,
        )?;

        seller_p2.spend(
            &mut ctx,
            funding_coin,
            conditions
                .extend(launch)
                .create_coin(seller.puzzle_hash, change, Memos::None),
        )?;

        let coin_spends = ctx.take();
        sim.spend_coins(coin_spends.clone(), std::slice::from_ref(&seller.sk))?;
        assert_mempool_valid(&coin_spends)?;

        let launcher_spend = sim
            .coin_spend(auction.info.launcher_id)
            .ok_or(anyhow!("missing launcher spend"))?;
        let launcher_solution = launcher_spend.solution.to_clvm(&mut ctx)?;
        let parsed = parse_auction_launch(&ctx, launcher_spend.coin, launcher_solution)?;
        assert_eq!(parsed, Some(auction));

        Ok(Self {
            sim,
            ctx,
            config,
            seller,
            auction,
            nft: locked_nft,
            bank,
        })
    }

    /// Checks that `coin_spends` passes mempool rules, and that the auction's new state can be parsed
    /// from the blockchain.
    fn confirmed(&mut self, coin_spends: &[CoinSpend], auction: Auction) -> Result<()> {
        assert_mempool_valid(coin_spends)?;

        let coin_spend = self
            .sim
            .coin_spend(self.auction.coin.coin_id())
            .ok_or(anyhow!("missing auction spend"))?;
        let solution = coin_spend.solution.to_clvm(&mut self.ctx)?;
        let parsed = self.auction.parse_child(&mut self.ctx, solution)?;
        assert_eq!(parsed, auction);

        self.auction = auction;
        Ok(())
    }

    pub fn set_time(&mut self, time: u64) -> Result<()> {
        self.sim.set_next_timestamp(time)?;
        Ok(())
    }

    pub fn bid(&mut self, amount: u64) -> Result<Bidder> {
        self.bid_with_grace(amount, false)
    }

    pub fn bid_with_grace(&mut self, amount: u64, grace: bool) -> Result<Bidder> {
        Ok(self.bids(&[(amount, grace)])?[0])
    }

    /// Places every bid in a single spend of the auction. Each bidder funds exactly the reserve
    /// amount for their bid, and asserts the bid's announcement so the funds can't be reused.
    pub fn bids(&mut self, bids: &[(u64, bool)]) -> Result<Vec<Bidder>> {
        let result = self.try_bids(bids);
        if result.is_err() {
            self.ctx.take();
        }
        result
    }

    fn try_bids(&mut self, bids: &[(u64, bool)]) -> Result<Vec<Bidder>> {
        let mut bidders = Vec::new();
        let mut actions = Vec::new();
        let mut cat_spends = Vec::new();
        let mut keys = Vec::new();

        for &(amount, grace) in bids {
            let reserve_amount = self
                .auction
                .info
                .reserve_amount_for_bid(amount)
                .ok_or(anyhow!("reserve amount overflow"))?;

            let key = if self.bank.is_some() {
                self.sim.bls(0)
            } else {
                self.sim.bls(reserve_amount)
            };

            let bid = Bid::new(amount, key.puzzle_hash);
            actions.push(self.auction.spend_bid_action(&mut self.ctx, bid, grace)?);

            let conditions = Conditions::new().assert_coin_announcement(announcement_id(
                self.auction.coin.coin_id(),
                bid.tree_hash(),
            ));
            let p2 = StandardLayer::new(key.pk);

            if self.bank.is_some() {
                let cat = self.fund_cat(key.puzzle_hash, reserve_amount)?;
                let spend = p2.spend_with_conditions(&mut self.ctx, conditions)?;
                cat_spends.push(CatSpend::new(cat, spend));
            } else {
                p2.spend(&mut self.ctx, key.coin, conditions)?;
            }

            keys.push(key.sk.clone());
            bidders.push(Bidder {
                bid,
                reserve_amount,
            });
        }

        let auction = self.auction.spend(&mut self.ctx, actions, cat_spends)?;
        let coin_spends = self.ctx.take();
        self.sim.spend_coins(coin_spends.clone(), &keys)?;
        self.confirmed(&coin_spends, auction)?;

        Ok(bidders)
    }

    /// Ends the auction and unlocks the NFT, returning the NFT sent to the winning bidder.
    pub fn end(&mut self) -> Result<Nft> {
        let result = self.try_end();
        if result.is_err() {
            self.ctx.take();
        }
        result
    }

    fn try_end(&mut self) -> Result<Nft> {
        let end_action = self.auction.spend_end_action(&mut self.ctx, &self.nft)?;
        let nft = self.auction.unlock_nft(&mut self.ctx, &self.nft)?;
        let auction = self
            .auction
            .spend(&mut self.ctx, vec![end_action], vec![])?;
        let coin_spends = self.ctx.take();
        self.sim.spend_coins(coin_spends.clone(), &[])?;
        self.confirmed(&coin_spends, auction)?;
        self.nft = nft;
        Ok(nft)
    }

    /// Creates an empty coin at the auction lock, which can be spent as the reserve in place of the
    /// real one while the auction's reserve amount is 0.
    pub fn decoy_reserve(&mut self) -> Result<AuctionReserve> {
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(self.auction.info.launcher_id);

        if self.bank.is_some() {
            return Ok(AuctionReserve::Cat(self.fund_cat(lock_puzzle_hash, 0)?));
        }

        let key = self.sim.bls(0);
        StandardLayer::new(key.pk).spend(
            &mut self.ctx,
            key.coin,
            Conditions::new().create_coin(lock_puzzle_hash, 0, Memos::None),
        )?;
        self.sim
            .spend_coins(self.ctx.take(), std::slice::from_ref(&key.sk))?;

        Ok(AuctionReserve::Xch(Coin::new(
            key.coin.coin_id(),
            lock_puzzle_hash,
            0,
        )))
    }

    fn fund_cat(&mut self, puzzle_hash: Bytes32, amount: u64) -> Result<Cat> {
        let (key, cat) = self.bank.clone().ok_or(anyhow!("no CAT bank"))?;
        let change = cat.coin.amount - amount;

        let spend = StandardLayer::new(key.pk).spend_with_conditions(
            &mut self.ctx,
            Conditions::new()
                .create_coin(puzzle_hash, amount, Memos::None)
                .create_coin(key.puzzle_hash, change, Memos::None),
        )?;
        Cat::spend_all(&mut self.ctx, &[CatSpend::new(cat, spend)])?;
        self.sim
            .spend_coins(self.ctx.take(), std::slice::from_ref(&key.sk))?;

        let bank_cat = cat.child(key.puzzle_hash, change);
        self.bank = Some((key, bank_cat));
        Ok(cat.child(puzzle_hash, amount))
    }

    /// A coin created by the reserve coin `reserve` with the given puzzle hash and amount.
    pub fn reserve_output(
        &self,
        reserve: AuctionReserve,
        puzzle_hash: Bytes32,
        amount: u64,
    ) -> Coin {
        match reserve {
            AuctionReserve::Xch(coin) => Coin::new(coin.coin_id(), puzzle_hash, amount),
            AuctionReserve::Cat(cat) => cat.child(puzzle_hash, amount).coin,
        }
    }

    /// The royalty payment made by the settlement coin that the reserve coin `reserve` creates.
    pub fn royalty_output(&self, reserve: AuctionReserve, amount: u64) -> Coin {
        match reserve {
            AuctionReserve::Xch(coin) => {
                let settlement = Coin::new(coin.coin_id(), SETTLEMENT_PAYMENT_HASH.into(), amount);
                Coin::new(settlement.coin_id(), ROYALTY_PUZZLE_HASH, amount)
            }
            AuctionReserve::Cat(cat) => {
                cat.child(SETTLEMENT_PAYMENT_HASH.into(), amount)
                    .child(ROYALTY_PUZZLE_HASH, amount)
                    .coin
            }
        }
    }

    /// Asserts that the reserve coin `reserve` paid out a winning bid of `amount` to the seller and
    /// every fee recipient, and created an empty reserve for the ended auction.
    pub fn assert_settled(&self, reserve: AuctionReserve, amount: u64) {
        let premium = fee(amount, self.config.buyers_premium_bps);
        let commission = fee(amount, self.config.commission_bps);
        let royalty = fee(amount, self.config.royalty_bps.into());
        let payout = amount - commission;

        assert_eq!(reserve.coin().amount, amount + premium + royalty);

        for (puzzle_hash, payment) in [
            (PREMIUM_PUZZLE_HASH, premium),
            (COMMISSION_PUZZLE_HASH, commission),
            (self.seller.puzzle_hash, payout),
        ] {
            let coin = self.reserve_output(reserve, puzzle_hash, payment);
            if payment > 0 {
                self.assert_unspent(coin);
            } else {
                self.assert_missing(coin);
            }
        }

        if royalty > 0 {
            self.assert_unspent(self.royalty_output(reserve, royalty));
        } else {
            self.assert_missing(self.reserve_output(reserve, SETTLEMENT_PAYMENT_HASH.into(), 0));
        }

        let child_reserve = self.auction.info.reserve.coin();
        assert_eq!(child_reserve.parent_coin_info, reserve.coin().coin_id());
        assert_eq!(child_reserve.amount, 0);
        self.assert_unspent(child_reserve);
    }

    pub fn assert_unspent(&self, coin: Coin) {
        let state = self
            .sim
            .coin_state(coin.coin_id())
            .unwrap_or_else(|| panic!("expected coin {coin:?} to exist"));
        assert!(
            state.spent_height.is_none(),
            "expected coin {coin:?} to be unspent"
        );
    }

    pub fn assert_missing(&self, coin: Coin) {
        assert!(
            self.sim.coin_state(coin.coin_id()).is_none(),
            "expected coin {coin:?} not to exist"
        );
    }
}

/// The simulator only enforces consensus rules, which are more lenient than the mempool's.
pub fn assert_mempool_valid(coin_spends: &[CoinSpend]) -> Result<()> {
    let mut allocator = Allocator::new();
    let spend_bundle = SpendBundle::new(coin_spends.to_vec(), Signature::default());
    get_conditions_from_spendbundle(
        &mut allocator,
        &spend_bundle,
        TEST_CONSTANTS.max_block_cost_clvm,
        u32::MAX,
        &TEST_CONSTANTS,
    )
    .map_err(|error| anyhow!("mempool validation failed: {error:?}"))?;
    Ok(())
}

pub fn fee(amount: u64, bps: u64) -> u64 {
    calculate_bps_payment(amount, bps).unwrap()
}

/// Asserts that `result` failed, and that the error message contains `expected`.
pub fn assert_fails_with<T: Debug>(result: Result<T>, expected: &str) {
    match result {
        Ok(value) => panic!("expected an error containing {expected:?}, got {value:?}"),
        Err(error) => {
            let message = format!("{error:?}");
            assert!(
                message.contains(expected),
                "expected an error containing {expected:?}, got {message}"
            );
        }
    }
}
