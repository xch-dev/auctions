mod actions;
mod auction;

pub use actions::*;
pub use auction::*;

#[cfg(test)]
mod tests {
    use anyhow::Result;

    #[test]
    fn test_auction() -> Result<()> {
        Ok(())
    }
}
