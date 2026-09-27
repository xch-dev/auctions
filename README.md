# Auction Primitive

An implementation of English auctions for the Chia blockchain. Puzzles are written in [Rue](https://rue-lang.com) and drivers are written in [Rust](https://rust-lang.org/).

An auction is a singleton that locks up an NFT until it ends, and allows bids (in the form of XCH, CATs, or Revocable CATs) to be placed on it.

## Life Cycle

The life cycle of an auction is as follows:

1. The auctioneer locks up an NFT to the auction's lock address, and launches the auction singleton before or in the same spend bundle. The auction is tied to the locked NFT's coin, and sends it back to the auctioneer's payout address if there are no bids.
2. Until the auction is ended, anyone can place a bid as long as their bid is strictly higher than the current winning bid and meets certain requirements which can be specified by the auctioneer. The bidder locks up their bid plus the buyer's premium and NFT royalty, and the previous bidder is refunded everything they locked up. Multiple bids can be placed in a single spend.
3. When the auction is ended, the NFT is sent to the highest bidder, the auctioneer receives the final bid amount minus the commission, and the buyer's premium, commission, and NFT royalty are paid out. If there were no bids, the NFT is sent back to the auctioneer. An auction can only be ended once, and no bids can be placed after it's ended.

## Customization

The following parameters can be customized by the auctioneer:

- The minimum starting bid
- The minimum bid increment (flat or percentage)
- The minimum amount of time before the auction is ended
- The grace period after a bid is placed before the auction is ended

These can also be customized, but they may be enforced to be certain values by the auction house and/or wallet, to ensure that they get a cut:

- The buyer's premium
- The commission

Both the buyer's premium and commission are percentages, and are sent to a single address. If these need to be split between multiple addresses, you should use something external to do so such as the [royalty split](https://splitxch.com/) primitive.

## Fees and Royalties

All fees are specified in basis points (1/100th of a percent), and are rounded down. For a winning bid of `bid`:

- The **buyer's premium** is paid by the bidder on top of their bid, and is sent to the buyer's premium address.
- The **commission** is taken out of the bid, and is sent to the commission address. The auctioneer receives the rest of the bid.
- The **NFT royalty** is paid by the bidder on top of their bid, using the royalty address and basis points of the NFT itself. It's paid through the settlement payments puzzle, in the same way as an offer trade, so the NFT's royalty requirements are met when it's transferred. If the royalty rounds down to 0 (including when there are no bids), the NFT is transferred without a trade price and no royalty is paid.

So each bidder locks up `bid + floor(bid * premium / 10000) + floor(bid * royalty / 10000)`, which is available from `AuctionInfo::reserve_amount_for_bid`. Payments that round down to 0 are skipped.

## Discovery

The auction launcher's memo (`AuctionMemo`) contains the auction's settings, the locked NFT's coin ID, and the reserve coin's parent (plus the asset ID, hidden puzzle hash, and lineage proof for CATs). The reserve is always an empty coin locked by the auction at launch, so nothing else is needed to find it. The NFT's royalty info is curried into the auction but isn't in the memo, so wallets fetch it from the NFT on the blockchain.

- `AuctionLaunch::parse` reads an auction's launcher spend. It returns `None` for launchers that aren't auctions.
- `AuctionLaunch::into_auction` reconstructs the auction, given the royalty info of the NFT at `AuctionLaunch::nft_coin_id`. It returns an error for invalid auctions, and for auctions that don't match the launched singleton. That includes an NFT whose royalty basis points don't match the auction's, since they're curried in.
- `AuctionExt::parse_child` follows an auction from one spend to the next, given the solution of a confirmed spend. It runs the action puzzles in the solution without checking them, so it must not be used on unconfirmed spends.

Auctions must be launched with a singleton amount of 1, because the reserve finalizer always recreates the singleton with that amount.

The reserve finalizer takes the reserve coin's parent from the singleton's solution, so any coin at the lock address with the reserve's amount (and asset, for CATs) can be spent as the reserve. The new reserve is the child of whichever coin was spent, which `parse_child` reads from the solution. Wallets should never find the reserve by following the previous reserve's children. This can't be used to take funds, since the spent coin must hold exactly the reserve amount and its outputs are fixed by the actions.

## Validation

The puzzles can't prevent every misconfiguration on their own, so `AuctionLaunch::into_auction` rejects invalid auctions. `launch_auction` runs the same checks, but not the ones that are left to wallets below, so sellers are responsible for locking the NFT and reserve correctly:

- The minimum bid and bid increment must be greater than 0.
- The buyer's premium, commission, and NFT royalty can't exceed 10,000 basis points.
- Addresses that receive a payment must be unique, and can't be the settlement payments puzzle or the auction's own lock address. Addresses that aren't paid (with 0 basis points) are ignored.

Some things can't be checked offline, so before bidding on an auction, wallets must also:

- Check the locked NFT: its coin must be the auction's NFT coin ID and unspent, and it must be locked by the auction (its p2 puzzle hash is `auction_lock_p2_puzzle_hash` of the launcher ID). Its royalty info must be the one passed to `into_auction`.
- Check the reserve. The singleton trusts the reserve to output the conditions it asks for, so it must be locked by the auction, or it could keep the bids. `into_auction` always rebuilds the reserve at the lock address, so a parsed auction whose singleton matches its memo has a locked reserve. For CAT reserves, wallets must also check that the reserve's lineage is valid. The launcher memo's reserve doesn't need to exist before the first bid, since the first bidder can create an empty coin at the lock address and spend it as the reserve instead.
- Show the timings to the user. They aren't limited, so an auction could end far in the future, or have a grace period so long that it never ends in practice once it has a bid.

## Competing Bids

Every bid spends the current auction coin, so two separate bids on the same auction conflict in the mempool. The mempool only replaces a pending spend bundle with one that spends all of the same coins and pays a higher fee, so a standalone bid is rejected until the pending one confirms.

To outbid a pending bid, wallets should take its spend bundle and add their own bid after it in the same auction spend, aggregating their signature and paying a higher fee. The pending bidder is refunded in full in the same spend, so this is safe for them.

Near the end time, this means that a bid landing in the last block can't be answered unless the auction has a grace period, so wallets should default to a nonzero grace period when creating auctions.

## Trust Assumptions

- **Revocable CATs:** The issuer of a revocable CAT can spend any coin of that CAT, including the reserve. If they revoke the reserve during an auction, they take the current winning bid, and the auction can never be ended, so the NFT is locked forever. Wallets should make it clear that the issuer is trusted when an auction's reserve is a revocable CAT.
- **Grace period flag:** Each bidder's coin asserts an announcement of their bid (its amount and puzzle hash), but not whether it was placed using the grace period. Anyone relaying a pending bid can change that flag, which only changes when the bid is allowed to land. The bidder still pays exactly what they bid.

## Development

The compiled puzzles are checked into the repository next to their sources (as `.rue.hex` and `.rue.hash` files), and are embedded into the driver, so that the puzzle hashes never change without a corresponding change in version control. After changing a puzzle, rebuild them with [Rue](https://rue-lang.com) (the version in `Rue.toml`):

```bash
rue build -a -s -x -h
```

CI should run the same command followed by `git diff --exit-code puzzles` to ensure that the compiled puzzles are up to date. Then run the tests with:

```bash
cargo test
```

## Credits

- [Josh Painter](https://github.com/joshpainter) - for the original [draft CHIP](https://github.com/Chia-Network/chips/pull/24) for auctions and its high level design.
- [Yakuhito](https://github.com/yakuhito) - for creating the [Action Layer](https://github.com/Chia-Network/chips/pull/165) primitive which is use to implement the auction standard, and for helping design the primitive.
