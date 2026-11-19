//! Pure helpers for inline recipe editing.
//!
//! Kept free of Dioxus so the parsing rules can be unit-tested on the host with
//! `cargo test --no-default-features`.

use cooklang::Value;
use cooklang::quantity::Number;

/// Parse an amount typed by the user: `2`, `1/2`, `2 1/2`, or a range `5-8`.
///
/// Only positive values are accepted, and a range must not decrease. Ingredients
/// keep their authored unit, so units are intentionally not parsed here.
pub fn parse_value(input: &str) -> Option<Value> {
    let input = input.trim();
    if let Some((start, end)) = input.split_once('-').or_else(|| input.split_once('–')) {
        let start = parse_number(start)?;
        let end = parse_number(end)?;
        return (start > 0.0 && end >= start).then_some(Value::Range {
            start: Number::Regular(start),
            end: Number::Regular(end),
        });
    }
    let value = parse_number(input)?;
    (value > 0.0).then_some(Value::Number(Number::Regular(value)))
}

fn parse_number(input: &str) -> Option<f64> {
    let input = input.trim();
    if let Some((whole, fraction)) = input.split_once(' ') {
        let whole = whole.trim().parse::<f64>().ok()?;
        return Some(whole + parse_fraction(fraction)?);
    }
    if input.contains('/') {
        return parse_fraction(input);
    }
    input.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn parse_fraction(input: &str) -> Option<f64> {
    let (numerator, denominator) = input.trim().split_once('/')?;
    let numerator = numerator.trim().parse::<f64>().ok()?;
    let denominator = denominator.trim().parse::<f64>().ok()?;
    (denominator != 0.0).then_some(numerator / denominator)
}

/// Format a duration in seconds as minutes, without trailing zeros.
pub fn format_minutes(secs: u64) -> String {
    let minutes = secs as f64 / 60.0;
    if minutes.fract().abs() < f64::EPSILON {
        format!("{}", minutes.round() as u64)
    } else {
        format!("{minutes:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(input: &str) -> Option<f64> {
        match parse_value(input) {
            Some(Value::Number(number)) => Some(number.value()),
            _ => None,
        }
    }

    #[test]
    fn parses_decimals_fractions_and_mixed_numbers() {
        assert_eq!(number("2"), Some(2.0));
        assert_eq!(number("1.5"), Some(1.5));
        assert_eq!(number(" 3 "), Some(3.0));
        assert_eq!(number("1/2"), Some(0.5));
        assert_eq!(number("2 1/2"), Some(2.5));
    }

    #[test]
    fn parses_ranges() {
        let Some(Value::Range { start, end }) = parse_value("5-8") else {
            panic!("expected a range");
        };
        assert_eq!((start.value(), end.value()), (5.0, 8.0));

        // An en dash is accepted too, since keyboards may produce one.
        let Some(Value::Range { start, end }) = parse_value("10 – 15") else {
            panic!("expected a range");
        };
        assert_eq!((start.value(), end.value()), (10.0, 15.0));
    }

    #[test]
    fn rejects_zero_negative_and_malformed_values() {
        assert_eq!(parse_value(""), None);
        assert_eq!(parse_value("0"), None);
        assert_eq!(parse_value("-2"), None);
        assert_eq!(parse_value("8-5"), None);
        assert_eq!(parse_value("two"), None);
        assert_eq!(parse_value("1/0"), None);
        assert_eq!(parse_value("NaN"), None);
    }

    #[test]
    fn formats_whole_and_fractional_minutes() {
        assert_eq!(format_minutes(600), "10");
        assert_eq!(format_minutes(60), "1");
        assert_eq!(format_minutes(90), "1.5");
        assert_eq!(format_minutes(30), "0.5");
        assert_eq!(format_minutes(45), "0.75");
    }
}
