//! JSONC (JSON-with-comments) parsing adapter.
//!
//! `devcontainer.json` files are conventionally JSONC: standard JSON plus `//` and
//! `/* */` comments and optional trailing commas. This module wraps the
//! [`jsonc-parser`](https://docs.rs/jsonc-parser) crate and translates its AST into
//! [`serde_json::Value`] so the rest of the codebase can keep consuming `Value`s
//! unchanged.
//!
//! Object key order is preserved end-to-end: `jsonc-parser` is compiled with the
//! `preserve_order` feature (its `JsonObject` is backed by an `IndexMap`), and the
//! conversion inserts entries into a `serde_json::Map` in encounter order. The
//! `serde_json` crate in this workspace is built with `preserve_order`, so the
//! `Map` itself is also order-preserving.

use crate::errors::{ConfigError, DeaconError, Result};
use jsonc_parser::{JsonValue, ParseOptions, parse_to_value};
use serde_json::{Map, Number, Value};

/// Parse a JSONC string into a `serde_json::Value`.
///
/// Accepts the full set of JSONC conveniences (single-line and block comments,
/// trailing commas). JSON5-only sugar (single-quoted strings, hexadecimal
/// numbers, unary `+`, unquoted property names) is also tolerated by the
/// underlying parser's default options; real-world `devcontainer.json` files
/// do not use it, but we keep the permissive defaults to match the prior
/// `json5`-based behaviour.
///
/// Returns `ConfigError::Parsing` on malformed input or on a number literal
/// that cannot be represented as a `serde_json::Number`.
pub fn parse(text: &str) -> Result<Value> {
    let parsed = parse_to_value(text, &ParseOptions::default()).map_err(|e| {
        DeaconError::Config(ConfigError::Parsing {
            message: format!("JSONC parsing error: {}", e),
        })
    })?;

    match parsed {
        Some(value) => convert(value),
        None => Ok(Value::Null),
    }
}

fn convert(value: JsonValue<'_>) -> Result<Value> {
    Ok(match value {
        JsonValue::Null => Value::Null,
        JsonValue::Boolean(b) => Value::Bool(b),
        JsonValue::String(s) => Value::String(s.into_owned()),
        JsonValue::Number(raw) => Value::Number(parse_number(raw)?),
        JsonValue::Array(arr) => {
            let elems = arr.take_inner();
            let mut out = Vec::with_capacity(elems.len());
            for el in elems {
                out.push(convert(el)?);
            }
            Value::Array(out)
        }
        JsonValue::Object(obj) => {
            let mut map = Map::with_capacity(obj.len());
            for (k, v) in obj.into_iter() {
                map.insert(k, convert(v)?);
            }
            Value::Object(map)
        }
    })
}

fn parse_number(raw: &str) -> Result<Number> {
    // Strip optional unary plus, accepted by jsonc-parser with default options.
    let stripped = raw.strip_prefix('+').unwrap_or(raw);

    // Hexadecimal: jsonc-parser tolerates `0x...` / `-0x...`. JSON itself
    // does not, but we preserve permissive semantics by converting to i64.
    let (sign, hex_body) = if let Some(rest) = stripped
        .strip_prefix("-0x")
        .or_else(|| stripped.strip_prefix("-0X"))
    {
        (-1i128, Some(rest))
    } else if let Some(rest) = stripped
        .strip_prefix("0x")
        .or_else(|| stripped.strip_prefix("0X"))
    {
        (1i128, Some(rest))
    } else {
        (1i128, None)
    };
    if let Some(body) = hex_body {
        if let Ok(magnitude) = i128::from_str_radix(body, 16) {
            let signed = sign
                .checked_mul(magnitude)
                .ok_or_else(|| number_error(raw))?;
            if let Ok(v) = i64::try_from(signed) {
                return Ok(Number::from(v));
            }
            if signed >= 0 {
                if let Ok(v) = u64::try_from(signed) {
                    return Ok(Number::from(v));
                }
            }
        }
        return Err(number_error(raw));
    }

    if let Ok(v) = stripped.parse::<i64>() {
        return Ok(Number::from(v));
    }
    if let Ok(v) = stripped.parse::<u64>() {
        return Ok(Number::from(v));
    }
    if let Ok(v) = stripped.parse::<f64>() {
        if let Some(n) = Number::from_f64(v) {
            return Ok(n);
        }
    }
    Err(number_error(raw))
}

fn number_error(raw: &str) -> DeaconError {
    DeaconError::Config(ConfigError::Parsing {
        message: format!("Invalid number literal in configuration: {}", raw),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_json() {
        let value = parse(r#"{"a": 1, "b": "two", "c": true}"#).unwrap();
        assert_eq!(value["a"], 1);
        assert_eq!(value["b"], "two");
        assert_eq!(value["c"], true);
    }

    #[test]
    fn parses_comments_and_trailing_commas() {
        let value = parse(
            r#"{
                // line comment
                "name": "demo", /* block */
                "list": [1, 2, 3,],
            }"#,
        )
        .unwrap();
        assert_eq!(value["name"], "demo");
        assert_eq!(value["list"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn preserves_object_key_order() {
        let value = parse(r#"{"z": 1, "a": 2, "m": 3}"#).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(keys, vec!["z", "a", "m"]);
    }

    #[test]
    fn empty_input_yields_null() {
        let value = parse("").unwrap();
        assert_eq!(value, Value::Null);
    }

    #[test]
    fn reports_parse_errors() {
        let err = parse("{not valid").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("JSONC parsing error"), "got: {}", msg);
    }

    #[test]
    fn numbers_match_serde_json_for_common_shapes() {
        let cases = [
            ("0", Value::from(0u64)),
            ("-1", Value::from(-1i64)),
            ("2.5", serde_json::json!(2.5)),
            ("1e10", serde_json::json!(1e10)),
        ];
        for (input, expected) in cases {
            let got = parse(&format!("{{\"n\": {}}}", input)).unwrap();
            assert_eq!(got["n"], expected, "input: {}", input);
        }
    }
}
