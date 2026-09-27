/// Implements [`Mod`](chia_wallet_sdk::types::Mod) using the `.hex` and `.hash` files that
/// `rue build -a -s -x -h` writes next to each puzzle, so that the puzzles never need to be compiled
/// at runtime and their hashes are pinned in the repository.
macro_rules! include_puzzle {
    ( $args:ident $(< $($generic:ident),+ >)? = $name:ident, $path:literal ) => {
        static $name: ::std::sync::LazyLock<(
            ::std::vec::Vec<u8>,
            ::chia_wallet_sdk::prelude::TreeHash,
        )> = ::std::sync::LazyLock::new(|| {
            // The files are line wrapped
            let reveal = ::hex::decode(
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path, ".hex"))
                    .split_whitespace()
                    .collect::<String>(),
            )
            .expect(concat!("invalid hex in ", $path, ".hex"));

            let hash: [u8; 32] = ::hex::decode(
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path, ".hash"))
                    .split_whitespace()
                    .collect::<String>(),
            )
            .ok()
            .and_then(|hash| hash.try_into().ok())
            .expect(concat!("invalid hash in ", $path, ".hash"));

            (reveal, ::chia_wallet_sdk::prelude::TreeHash::new(hash))
        });

        impl$(<$($generic),+>)? ::chia_wallet_sdk::types::Mod for $args$(<$($generic),+>)? {
            fn mod_reveal() -> ::std::borrow::Cow<'static, [u8]> {
                ::std::borrow::Cow::Borrowed(&$name.0)
            }

            fn mod_hash() -> ::chia_wallet_sdk::prelude::TreeHash {
                $name.1
            }
        }
    };
}

pub(crate) use include_puzzle;
