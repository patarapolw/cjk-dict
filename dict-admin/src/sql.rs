//! SQL literal helpers. Statements are written to files and run by `wrangler d1 execute`,
//! so values are inlined as literals rather than bound.

pub fn s(v: &str) -> String {
    // NUL can't appear in a SQLite text literal
    let v = v.replace('\0', "");
    format!("'{}'", v.replace('\'', "''"))
}

pub fn opt_s(v: Option<&str>) -> String {
    match v {
        Some(v) => s(v),
        None => "NULL".to_string(),
    }
}

pub fn num(v: f64) -> String {
    if !v.is_finite() {
        "NULL".to_string()
    } else if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

pub fn opt_num(v: Option<f64>) -> String {
    v.map(num).unwrap_or_else(|| "NULL".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(s("it's"), "'it''s'");
        assert_eq!(s("a\0b"), "'ab'");
        assert_eq!(num(3.0), "3");
        assert_eq!(num(-1.5), "-1.5");
        assert_eq!(num(f64::NAN), "NULL");
    }
}
