use auction::*;
use chia_wallet_sdk::{clvmr::serde::node_from_bytes, prelude::*, types::Mod};

fn assert_mod<T: Mod>(name: &str) {
    let mut allocator = Allocator::new();
    let reveal = T::mod_reveal();
    let node = node_from_bytes(&mut allocator, &reveal)
        .unwrap_or_else(|error| panic!("{name} isn't valid CLVM: {error}"));
    assert_eq!(
        tree_hash(&allocator, node),
        T::mod_hash(),
        "{name} doesn't match its hash"
    );
}

#[test]
fn embedded_puzzles_match_their_hashes() {
    assert_mod::<BidActionArgs<TreeHash>>("bid_action");
    assert_mod::<EndActionArgs<TreeHash>>("end_action");
    assert_mod::<NftUnlockerArgs>("nft_unlocker");
    assert_mod::<FlatBidVerifierArgs>("flat_bid_verifier");
    assert_mod::<PercentBidVerifierArgs>("percent_bid_verifier");
}
