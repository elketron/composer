/// `${key}` in `text` replaced by `lookup(key)`; a key it does not know stays as written.
pub fn substitute(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        match lookup(&after[..end]) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::substitute;

    #[test]
    fn substitutes_known_keys_only() {
        let lookup = |k: &str| (k == "a").then(|| "1".to_string());
        assert_eq!(substitute("x${a}y${a}${b}${", &lookup), "x1y1${b}${");
    }
}
