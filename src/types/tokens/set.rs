//! Token set type for collections of unique token addresses

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Represents a set of unique token addresses
///
/// This type provides semantic meaning for collections of token addresses,
/// making it clear when a value represents a set of tokens rather than
/// an arbitrary collection of addresses.
///
/// Uses `BTreeSet` internally for:
/// - Automatic deduplication
/// - Deterministic ordering (important for testing and reproducibility)
///
/// # Examples
///
/// ```
/// use semioscan::TokenSet;
/// use alloy_primitives::address;
///
/// let mut tokens = TokenSet::new();
/// tokens.insert(address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")); // USDC
/// tokens.insert(address!("dac17f958d2ee523a2206206994597c13d831ec7")); // USDT
///
/// assert_eq!(tokens.len(), 2);
/// assert!(tokens.contains(&address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenSet(BTreeSet<Address>);

impl TokenSet {
    /// Create a new empty token set
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Insert a token address into the set
    ///
    /// Returns `true` if the token was newly inserted, `false` if it was already present.
    pub fn insert(&mut self, token: Address) -> bool {
        self.0.insert(token)
    }

    /// Check if a token address is in the set
    pub fn contains(&self, token: &Address) -> bool {
        self.0.contains(token)
    }

    /// Get the number of unique tokens in the set
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over token addresses in the set
    pub fn iter(&self) -> impl Iterator<Item = &Address> {
        self.0.iter()
    }

    /// Convert to inner BTreeSet (for compatibility with existing code)
    pub fn into_inner(self) -> BTreeSet<Address> {
        self.0
    }

    /// Get reference to inner BTreeSet
    pub fn as_inner(&self) -> &BTreeSet<Address> {
        &self.0
    }
}

impl Default for TokenSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FromIterator<Address> for TokenSet {
    fn from_iter<T: IntoIterator<Item = Address>>(iter: T) -> Self {
        Self(BTreeSet::from_iter(iter))
    }
}

impl IntoIterator for TokenSet {
    type Item = Address;
    type IntoIter = std::collections::btree_set::IntoIter<Address>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a TokenSet {
    type Item = &'a Address;
    type IntoIter = std::collections::btree_set::Iter<'a, Address>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl std::fmt::Display for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TokenSet({} tokens)", self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_set_creation() {
        let tokens = TokenSet::new();
        assert!(tokens.is_empty());
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_token_set_insert() {
        use alloy_primitives::address;

        let mut tokens = TokenSet::new();
        let usdc = address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
        let usdt = address!("dac17f958d2ee523a2206206994597c13d831ec7");

        // First insert returns true
        assert!(tokens.insert(usdc));
        assert_eq!(tokens.len(), 1);

        // Duplicate insert returns false
        assert!(!tokens.insert(usdc));
        assert_eq!(tokens.len(), 1);

        // New insert returns true
        assert!(tokens.insert(usdt));
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_token_set_contains() {
        use alloy_primitives::address;

        let mut tokens = TokenSet::new();
        let usdc = address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");

        assert!(!tokens.contains(&usdc));
        tokens.insert(usdc);
        assert!(tokens.contains(&usdc));
    }

    #[test]
    fn test_token_set_from_iter() {
        use alloy_primitives::address;

        let addresses = vec![
            address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"), // USDC
            address!("dac17f958d2ee523a2206206994597c13d831ec7"), // USDT
            address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"), // USDC (duplicate)
        ];

        let tokens: TokenSet = addresses.into_iter().collect();
        assert_eq!(tokens.len(), 2); // Deduplication
    }

    #[test]
    fn test_token_set_iteration() {
        use alloy_primitives::address;

        let mut tokens = TokenSet::new();
        tokens.insert(address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"));
        tokens.insert(address!("dac17f958d2ee523a2206206994597c13d831ec7"));

        let collected: Vec<_> = tokens.iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_token_set_into_iter() {
        use alloy_primitives::address;

        let mut tokens = TokenSet::new();
        tokens.insert(address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"));
        tokens.insert(address!("dac17f958d2ee523a2206206994597c13d831ec7"));

        let collected: Vec<_> = tokens.into_iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_token_set_display() {
        let tokens = TokenSet::new();
        assert_eq!(format!("{}", tokens), "TokenSet(0 tokens)");

        let mut tokens = TokenSet::new();
        tokens.insert(Address::ZERO);
        assert_eq!(format!("{}", tokens), "TokenSet(1 tokens)");
    }

    #[test]
    fn test_token_set_serialization() {
        use alloy_primitives::address;

        let mut tokens = TokenSet::new();
        tokens.insert(address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"));

        let json = serde_json::to_string(&tokens).unwrap();
        let deserialized: TokenSet = serde_json::from_str(&json).unwrap();
        assert_eq!(tokens, deserialized);
    }

    #[test]
    fn test_token_set_default() {
        let tokens = TokenSet::default();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_token_set_into_inner() {
        use alloy_primitives::address;

        let mut tokens = TokenSet::new();
        let usdc = address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
        tokens.insert(usdc);

        let inner = tokens.into_inner();
        assert!(inner.contains(&usdc));
    }
}
