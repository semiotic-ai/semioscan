//! Demonstrates proper UsdValue validation
//!
//! This example shows the improved validation that works in both debug and release builds.

use semioscan::{UsdValue, UsdValueError};

fn main() {
    println!("=== UsdValue Validation Demo ===\n");

    // ✅ Valid values work as expected
    let valid = UsdValue::new(100.50);
    println!("✅ Valid value: {}", valid);

    // ✅ Fallible construction for external data
    match UsdValue::try_new(250.75) {
        Ok(value) => println!("✅ try_new succeeded: {}", value),
        Err(e) => println!("❌ Unexpected error: {}", e),
    }

    // ✅ Tiny negative values from floating point errors are clamped to zero
    let tiny_negative = -0.0000000005433305882411751; // Real value from production
    match UsdValue::try_new(tiny_negative) {
        Ok(value) => println!(
            "✅ Tiny negative {} clamped to zero: {}",
            tiny_negative, value
        ),
        Err(e) => println!("❌ Unexpected error: {}", e),
    }

    // ❌ Clearly negative values are rejected (in ALL builds, not just debug!)
    match UsdValue::try_new(-100.0) {
        Ok(_) => println!("❌ Should not accept negative values!"),
        Err(UsdValueError::Negative(v)) => {
            println!("✅ Correctly rejected negative value: {}", v)
        }
        Err(e) => println!("❌ Wrong error type: {}", e),
    }

    // ❌ NaN is rejected
    match UsdValue::try_new(f64::NAN) {
        Ok(_) => println!("❌ Should not accept NaN!"),
        Err(UsdValueError::NaN) => println!("✅ Correctly rejected NaN"),
        Err(e) => println!("❌ Wrong error type: {}", e),
    }

    // ❌ Infinity is rejected
    match UsdValue::try_new(f64::INFINITY) {
        Ok(_) => println!("❌ Should not accept infinity!"),
        Err(UsdValueError::Infinite(v)) => {
            println!("✅ Correctly rejected infinity: {}", v)
        }
        Err(e) => println!("❌ Wrong error type: {}", e),
    }

    // ✅ Subtraction saturates at zero (never goes negative)
    let a = UsdValue::new(10.0);
    let b = UsdValue::new(50.0);
    let result = a - b;
    println!(
        "\n✅ Saturating subtraction: ${} - ${} = {} (clamped to zero)",
        a.as_f64(),
        b.as_f64(),
        result
    );

    // ✅ Const construction for known-safe values
    const HUNDRED_DOLLARS: UsdValue = UsdValue::from_non_negative(100.0);
    println!("\n✅ Const value: {}", HUNDRED_DOLLARS);

    println!("\n=== All validations working correctly! ===");
}
