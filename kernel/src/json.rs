//! A minimal JSON value and writer. The kernel only ever emits JSON, so this
//! is all it needs, and it keeps the crate free of dependencies.

use std::fmt::Write;

#[derive(Clone, Debug)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Int(i64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn obj() -> Json {
        Json::Obj(Vec::new())
    }

    /// Adds a field to an object and returns it, for chaining.
    pub fn with(mut self, key: &str, value: impl Into<Json>) -> Json {
        if let Json::Obj(fields) = &mut self {
            fields.push((key.to_string(), value.into()));
        }
        self
    }

    /// Numbers rounded to `places` decimals, which keeps payloads compact
    /// without visible loss.
    pub fn round(x: f64, places: i32) -> Json {
        if !x.is_finite() {
            return Json::Null;
        }
        let k = 10f64.powi(places);
        Json::Num((x * k).round() / k)
    }

    pub fn nums(xs: &[f64], places: i32) -> Json {
        Json::Arr(xs.iter().map(|&x| Json::round(x, places)).collect())
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Json::Num(x) => {
                if !x.is_finite() {
                    out.push_str("null");
                } else if *x == x.trunc() && x.abs() < 1e15 {
                    let _ = write!(out, "{}", *x as i64);
                } else {
                    let _ = write!(out, "{x}");
                }
            }
            Json::Str(s) => {
                out.push('"');
                for c in s.chars() {
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
            Json::Arr(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    Json::Str(k.clone()).write(out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

impl From<f64> for Json {
    fn from(x: f64) -> Json {
        if x.is_finite() {
            Json::Num(x)
        } else {
            Json::Null
        }
    }
}
impl From<i64> for Json {
    fn from(x: i64) -> Json {
        Json::Int(x)
    }
}
impl From<i32> for Json {
    fn from(x: i32) -> Json {
        Json::Int(x as i64)
    }
}
impl From<usize> for Json {
    fn from(x: usize) -> Json {
        Json::Int(x as i64)
    }
}
impl From<bool> for Json {
    fn from(x: bool) -> Json {
        Json::Bool(x)
    }
}
impl From<&str> for Json {
    fn from(x: &str) -> Json {
        Json::Str(x.to_string())
    }
}
impl From<String> for Json {
    fn from(x: String) -> Json {
        Json::Str(x)
    }
}
impl From<Vec<Json>> for Json {
    fn from(x: Vec<Json>) -> Json {
        Json::Arr(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nested_values() {
        let j = Json::obj()
            .with("a", 1usize)
            .with("b", Json::round(1.23456, 2))
            .with("c", "x\"y\n")
            .with("d", Json::Arr(vec![Json::Null, true.into(), f64::NAN.into()]));
        assert_eq!(j.to_string(), r#"{"a":1,"b":1.23,"c":"x\"y\n","d":[null,true,null]}"#);
    }

    #[test]
    fn whole_numbers_have_no_fraction() {
        assert_eq!(Json::Num(3.0).to_string(), "3");
        assert_eq!(Json::Num(-0.5).to_string(), "-0.5");
    }
}
