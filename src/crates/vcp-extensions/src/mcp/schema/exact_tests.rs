// SPDX-License-Identifier: Apache-2.0
use super::{parse, Budget, Decimal, Error, Limits};
use std::cmp::Ordering;

fn budget() -> Budget {
    Budget::new(1_000_000, 65536)
}
fn decimal(token: &str) -> Decimal {
    Decimal::parse(token, &mut budget()).unwrap()
}
fn encoded(input: &str) -> String {
    String::from_utf8(
        parse(input.as_bytes(), Limits::default())
            .unwrap()
            .canonical_bytes()
            .to_vec(),
    )
    .unwrap()
}
fn error(input: &[u8], limits: Limits) -> Error {
    match parse(input, limits) {
        Err(error) => error,
        Ok(_) => panic!("expected rejected JSON"),
    }
}

#[test]
fn normalization_preserves_exact_values_and_integer_tokens() {
    for (expected, inputs) in [
        ("0", vec!["0", "-0", "-0.000", "0e128"]),
        ("1", vec!["1", "1.0", "1e0", "10e-1", "0.01e2", "1.00E+000"]),
        ("1e-1", vec!["0.1", "1e-1", "10e-2", "0.1000"]),
        ("-123e-2", vec!["-1.23", "-123e-2", "-0.0123e2"]),
        ("1000", vec!["1000", "1e3", "10000e-1"]),
    ] {
        for input in inputs {
            assert_eq!(decimal(input).token(), expected, "{input}");
            assert_eq!(encoded(input), expected, "{input}");
        }
    }
    for input in [
        "-9223372036854775808",
        "18446744073709551615",
        "9007199254740993",
        "18446744073709551616",
    ] {
        assert_eq!(encoded(input), input);
    }
    assert_ne!(encoded("9007199254740992"), encoded("9007199254740993"));
    assert_ne!(encoded("0.10000000000000000000000000001"), encoded("0.1"));
    assert_eq!(encoded("1e128"), "1e+128");
    assert_eq!(decimal("1e-128").token(), "1e-128");
}

#[test]
fn exact_integer_and_order_semantics() {
    for input in ["1.0", "-10e-1", "0.000", "100e-2", "1e128"] {
        assert!(decimal(input).is_integer());
    }
    for input in ["1e-1", "-1.01", "0.0001"] {
        assert!(!decimal(input).is_integer());
    }
    for (left, right, expected) in [
        ("0", "-0.0", Ordering::Equal),
        ("-0", "1e-128", Ordering::Less),
        ("-1e-128", "0", Ordering::Less),
        ("-100", "-99", Ordering::Less),
        ("-1.23", "-1.2300000000000000001", Ordering::Greater),
        ("12.3", "12.3001", Ordering::Less),
        ("1e128", "9e127", Ordering::Greater),
        (
            "18446744073709551616",
            "18446744073709551615",
            Ordering::Greater,
        ),
    ] {
        assert_eq!(
            decimal(left)
                .compare(&decimal(right), &mut budget())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn multiple_of_is_exact_without_floating_tolerance() {
    for (input, divisor, expected) in [
        ("0.3", "0.1", true),
        ("-0.3", "0.1", true),
        ("0", "0.1", true),
        ("0.30000000000000000000001", "0.1", false),
        ("1", "0.03", false),
        ("0.03", "1", false),
        ("1", "1e-128", true),
        ("1e-128", "1", false),
        ("18446744073709551616", "2", true),
        ("18446744073709551617", "2", false),
        ("0.0006", "0.0002", true),
        ("11.7", "0.9", true),
    ] {
        assert_eq!(
            decimal(input)
                .multiple_of(&decimal(divisor), &mut budget())
                .unwrap(),
            expected,
            "{input} / {divisor}"
        );
    }
    for divisor in ["0", "-0", "-1", "-0.1"] {
        assert_eq!(
            decimal("1").multiple_of(&decimal(divisor), &mut budget()),
            Err(Error::InvalidDivisor)
        );
    }
}

#[test]
fn long_coefficients_carries_remainders_and_normalization_boundaries() {
    let nines = "9".repeat(128);
    let nines_number = decimal(&nines);
    for divisor in ["3", "9"] {
        assert!(nines_number
            .multiple_of(&decimal(divisor), &mut budget())
            .unwrap());
    }
    assert!(!nines_number
        .multiple_of(&decimal("2"), &mut budget())
        .unwrap());
    assert_eq!(
        nines_number
            .compare(&decimal("1e128"), &mut budget())
            .unwrap(),
        Ordering::Less
    );
    assert!(!decimal("1e128")
        .multiple_of(&decimal("3"), &mut budget())
        .unwrap());
    // Independent decimal divisibility rules: digit sum2 and odd final digit.
    let power_plus_one = format!("1{}1", "0".repeat(126));
    for divisor in ["2", "3", "9"] {
        assert!(!decimal(&power_plus_one)
            .multiple_of(&decimal(divisor), &mut budget())
            .unwrap());
    }
    let max_coefficient = format!("1{}1", "2".repeat(126));
    for source in [
        format!("{max_coefficient}e128"),
        format!("{max_coefficient}e-128"),
        format!("{max_coefficient}e-127"),
        nines.clone(),
        "1e128".into(),
        "1e-128".into(),
    ] {
        let canonical = encoded(&source);
        assert_eq!(encoded(&canonical), canonical);
    }
    for (left, right) in [
        ("1e128".to_owned(), "10e127".to_owned()),
        ("1e-128".to_owned(), "0.1e-127".to_owned()),
        (max_coefficient.clone(), format!("{max_coefficient}0e-1")),
        (
            format!("{max_coefficient}e-127"),
            format!("{max_coefficient}0e-128"),
        ),
    ] {
        assert_eq!(encoded(&left), encoded(&right));
    }
    // Explicit exponent is itself bounded, even if coefficient reduction could
    // yield an otherwise representable result. Admission never expands its cap.
    assert_eq!(Decimal::parse("10e-129", &mut budget()), Err(Error::Bound));
}

#[test]
fn bounded_integer_oracle_checks_comparison_and_remainder() {
    // Independent rational oracle: all values represented as exact integer
    // millionths. No production normalization algorithm is duplicated.
    for a in -20i128..=20 {
        for ae in -3i32..=3 {
            let av = a * 10i128.pow((ae + 3) as u32);
            let left = decimal(&format!("{a}e{ae}"));
            for b in 1i128..=12 {
                for be in -3i32..=3 {
                    let bv = b * 10i128.pow((be + 3) as u32);
                    let right = decimal(&format!("{b}e{be}"));
                    assert_eq!(left.compare(&right, &mut budget()).unwrap(), av.cmp(&bv));
                    assert_eq!(
                        left.multiple_of(&right, &mut budget()).unwrap(),
                        av % bv == 0
                    );
                }
            }
        }
    }
}

#[test]
fn malformed_number_grammar_never_coerces() {
    for input in [
        "", "-", "+1", "01", "-01", "1.", ".1", "1e", "1e+", "1e-", "1e+-1", "1-2", "NaN",
        "Infinity", "1_0", "１２",
    ] {
        assert!(Decimal::parse(input, &mut budget()).is_err(), "{input}");
        assert!(
            parse(input.as_bytes(), Limits::default()).is_err(),
            "{input}"
        );
    }
}

#[test]
fn numeric_limits_and_shared_fuel_fail_closed() {
    assert!(Decimal::parse(&"9".repeat(128), &mut budget()).is_ok());
    assert_eq!(
        Decimal::parse(&"9".repeat(129), &mut budget()),
        Err(Error::Bound)
    );
    for input in ["1e129", "1e-129", "0e129", "1e9999999999999999999999"] {
        assert_eq!(Decimal::parse(input, &mut budget()), Err(Error::Bound));
    }
    assert_eq!(
        Decimal::parse(&"1".repeat(257), &mut budget()),
        Err(Error::Bound)
    );
    assert_eq!(
        Decimal::parse("12", &mut Budget::new(100, 1)),
        Err(Error::Bound)
    );
    assert_eq!(
        decimal("12.3").compare(&decimal("12.4"), &mut Budget::new(1, 1)),
        Err(Error::Bound)
    );
    assert_eq!(
        decimal("99999").multiple_of(&decimal("7"), &mut Budget::new(5, 1)),
        Err(Error::Bound)
    );
    assert_eq!(
        error(
            b"[12,34]",
            Limits {
                numeric_digits: 3,
                ..Limits::default()
            }
        ),
        Error::Bound
    );
    assert_eq!(
        error(
            b"1e127",
            Limits {
                bytes: 5,
                ..Limits::default()
            }
        ),
        Error::Bound
    );
    assert_eq!(
        error(
            b"{}",
            Limits {
                work: 1,
                ..Limits::default()
            }
        ),
        Error::Bound
    );
}

#[test]
fn json_strings_duplicates_and_private_keys_are_unambiguous() {
    assert_eq!(
        encoded(r#"{"z":1.0,"a":[true,null,"\u0061",{"b":0.3}]}"#),
        r#"{"a":[true,null,"a",{"b":3e-1}],"z":1}"#
    );
    assert_eq!(encoded(r#""\uD83D\uDC9A""#), "\"💚\"");
    for input in [r#"{"a":1,"\u0061":2}"#, r#"{"a":[{"b":0,"b":1}]}"#] {
        assert_eq!(error(input.as_bytes(), Limits::default()), Error::Duplicate);
    }
    for input in [
        r#"{"$serde_json::private::Number":"1.5"}"#,
        r#"{"v":[{"\u0024serde_json::private::Number":"1.5"}]}"#,
        r#"{"$serde_json::private::RawValue":"{\"x\":1}"}"#,
    ] {
        assert_eq!(error(input.as_bytes(), Limits::default()), Error::Reserved);
    }
    assert_eq!(
        encoded(r#"{"ordinary":"$serde_json::private::Number"}"#),
        r#"{"ordinary":"$serde_json::private::Number"}"#
    );
    for input in [
        r#""\uD800""#,
        r#""\q""#,
        "\"literal\nline\"",
        "[1,]",
        "{\"a\":1,}",
        "{}{}",
        "[true false]",
        "{a:1}",
    ] {
        assert!(
            parse(input.as_bytes(), Limits::default()).is_err(),
            "{input}"
        );
    }
    assert!(parse(&[b'"', 255, b'"'], Limits::default()).is_err());
}

#[test]
fn parser_stops_before_excess_children_and_bounds_output() {
    assert_eq!(
        error(
            b"[0,{malformed",
            Limits {
                nodes: 2,
                ..Limits::default()
            }
        ),
        Error::Bound
    );
    assert_eq!(
        error(
            b"[[{malformed",
            Limits {
                depth: 1,
                ..Limits::default()
            }
        ),
        Error::Bound
    );
    assert_eq!(
        error(b"{\"a\":0,\"a\":{malformed", Limits::default()),
        Error::Duplicate
    );
    assert_eq!(
        error(
            b"{\"$serde_json::private::Number\":{malformed",
            Limits::default()
        ),
        Error::Reserved
    );
    assert!(parse(
        b"[0]",
        Limits {
            depth: 1,
            nodes: 2,
            bytes: 3,
            ..Limits::default()
        }
    )
    .is_ok());
    assert_eq!(
        error(
            b" [0]",
            Limits {
                bytes: 3,
                ..Limits::default()
            }
        ),
        Error::Bound
    );
}

#[test]
fn value_canonical_capture_round_trips_remain_exact() {
    let parsed = parse(
        br#"{"n":9007199254740993.00000000001,"m":0.1,"x":18446744073709551616}"#,
        Limits::default(),
    )
    .unwrap();
    let value_copy = serde_json::to_value(parsed.value()).unwrap();
    let wire = serde_json::to_vec(&value_copy).unwrap();
    assert_eq!(wire, parsed.canonical_bytes());
    let restored: serde_json::Value = serde_json::from_slice(&wire).unwrap();
    assert_eq!(restored, value_copy);
    let parsed_again = parse(&wire, Limits::default()).unwrap();
    assert_eq!(parsed_again.canonical_bytes(), parsed.canonical_bytes());
    assert_eq!(
        parsed.value()["n"].to_string(),
        "900719925474099300000000001e-11"
    );
    assert!(parsed.value()["n"].is_number());
}
