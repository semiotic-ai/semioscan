//! Tests for PriceSource trait and implementations
//!
//! Tests focus on:
//! - Trait object safety (PriceSource can be used as Box<dyn PriceSource>)
//! - Event topic configuration
//!
//! Note: Swap extraction and filtering logic is tested via PriceCalculator
//! integration tests with MockPriceSource.

#[cfg(feature = "odos-example")]
use alloy_primitives::address;
#[cfg(feature = "odos-example")]
use semioscan::price::odos::OdosPriceSource;
#[cfg(feature = "odos-example")]
use semioscan::price::PriceSource;

#[test]
#[cfg(feature = "odos-example")]
fn test_odos_event_topics_not_empty() {
    // Verify that Odos price source provides event topics
    let router = address!("a669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13");
    let price_source = OdosPriceSource::new(router);

    let topics = price_source.event_topics();

    // Odos V2 has Swap and SwapMulti events
    assert!(!topics.is_empty(), "Event topics should not be empty");
    assert!(
        topics.len() >= 2,
        "Odos should have at least 2 event types (Swap, SwapMulti)"
    );
}

#[test]
#[cfg(feature = "odos-example")]
fn test_price_source_trait_object_safety() {
    // Verify that PriceSource is object-safe (can be used as Box<dyn PriceSource>)
    let router = address!("a669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13");
    let price_source = OdosPriceSource::new(router);

    // This compiles only if PriceSource is object-safe
    let _boxed: Box<dyn PriceSource> = Box::new(price_source);
}
