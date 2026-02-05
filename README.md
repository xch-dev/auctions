# Auction Primitive

An implementation of English auctions for the Chia blockchain. Puzzles are written in [Rue](https://rue-lang.com) and drivers are written in [Rust](https://rust-lang.org/).

An auction is a singleton that locks up an NFT until it ends, and allows bids (in the form of XCH, CATs, or Revocable CATs) to be placed on it.

## Life Cycle

The life cycle of an auction is as follows:

1. The auction singleton is launched by the auctioneer, who then locks up an NFT into it and specifies an address for the NFT to be sent back to if the auction is not fulfilled.
2. Until the auction is ended, anyone can place a bid as long as their bid is higher than the current winning bid and meets certain requirements which can be specified by the auctioneer. The previous bidder receives their bid back, and the amount in the reserves increases by the difference.
3. When the auction is ended, the NFT is sent to the highest bidder and the auctioneer receives the final bid amount minus the buyer's premium and commission, if applicable.

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

## Credits

- [Josh Painter](https://github.com/joshpainter) - for the original [draft CHIP](https://github.com/Chia-Network/chips/pull/24) for auctions and its high level design.
- [Yakuhito](https://github.com/yakuhito) - for creating the [Action Layer](https://github.com/Chia-Network/chips/pull/165) primitive which is use to implement the auction standard, and for helping design the primitive.
