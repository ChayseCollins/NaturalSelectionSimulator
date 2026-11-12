//! Minimal JSON writer for `headless` results (X4, M-17, M-18).
//!
//! A tiny value tree written with stable key order and two-space indentation. Floats use
//! Rust's shortest round-trip `Display`, so output is locale-independent and
//! deterministic; a non-finite float is written as `null`. `serde_json` is not used
//! because it is not among the pinned dependencies and this output is small and fixed.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    pub fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    pub fn uint(value: u64) -> Self {
        Self::Int(i128::from(value))
    }

    pub fn opt_f64(value: Option<f64>) -> Self {
        value.map_or(Self::Null, Self::Float)
    }

    pub fn opt_u64(value: Option<u64>) -> Self {
        value.map_or(Self::Null, Self::uint)
    }

    /// Pretty-printed text ending in a newline.
    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, indent: usize) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Int(value) => {
                let _ = write!(out, "{value}");
            }
            Self::Float(value) => {
                if value.is_finite() {
                    let value = if *value == 0.0 { 0.0 } else { *value };
                    let _ = write!(out, "{value}");
                } else {
                    out.push_str("null");
                }
            }
            Self::Str(value) => write_string(out, value),
            Self::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (k, item) in items.iter().enumerate() {
                    out.push_str(if k == 0 { "\n" } else { ",\n" });
                    push_indent(out, indent + 1);
                    item.write(out, indent + 1);
                }
                out.push('\n');
                push_indent(out, indent);
                out.push(']');
            }
            Self::Object(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (k, (key, value)) in fields.iter().enumerate() {
                    out.push_str(if k == 0 { "\n" } else { ",\n" });
                    push_indent(out, indent + 1);
                    write_string(out, key);
                    out.push_str(": ");
                    value.write(out, indent + 1);
                }
                out.push('\n');
                push_indent(out, indent);
                out.push('}');
            }
        }
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nested_values_with_escapes() {
        let value = Json::Object(vec![
            ("name", Json::str("A \"quoted\"\nline")),
            ("count", Json::uint(3)),
            ("mean", Json::Float(0.1)),
            ("none", Json::opt_f64(None)),
            ("flag", Json::Bool(true)),
            ("bad", Json::Float(f64::NAN)),
            ("list", Json::Array(vec![Json::Int(-1), Json::Float(-0.0)])),
            ("empty", Json::Object(vec![])),
        ]);
        assert_eq!(
            value.to_pretty_string(),
            "{\n  \"name\": \"A \\\"quoted\\\"\\nline\",\n  \"count\": 3,\n  \"mean\": 0.1,\n  \
             \"none\": null,\n  \"flag\": true,\n  \"bad\": null,\n  \"list\": [\n    -1,\n    0\n  ],\n  \
             \"empty\": {}\n}\n"
        );
    }
}
