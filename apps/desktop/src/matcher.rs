//! Picking from a list by typing: the query's characters in order (not necessarily together),
//! case-insensitive; tighter and earlier matches first.

/// How well `query` matches `text` (higher is better), or `None` if it doesn't.
pub fn score(query: &str, text: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut at = 0usize;
    let mut first = None;
    let mut gaps = 0i64;
    let mut last: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = (at..text.len()).find(|&i| text[i] == q)?;
        first.get_or_insert(found);
        if let Some(last) = last {
            gaps += (found - last - 1) as i64;
        }
        last = Some(found);
        at = found + 1;
    }
    let start = first.unwrap_or(0) as i64;
    // A match at a word's start beats one inside a word.
    let word_start = start == 0 || !text[start as usize - 1].is_alphanumeric();
    Some(1000 - gaps * 10 - start - if word_start { 0 } else { 50 })
}

/// The indexes of `items` matching `query`, best first (ties keep their order).
pub fn ranked<'a>(query: &str, items: impl Iterator<Item = &'a str>) -> Vec<usize> {
    let mut scored: Vec<(usize, i64)> = items
        .enumerate()
        .filter_map(|(i, text)| score(query, text).map(|s| (i, s)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequences_ranked() {
        assert!(score("nt", "New terminal").is_some());
        assert!(score("xyz", "New terminal").is_none());
        assert!(score("NEW", "new terminal").is_some(), "case-insensitive");
        assert_eq!(score("", "anything"), Some(0));
        let items = [
            "Toggle follow agent",
            "New terminal",
            "terminal close",
            "plans",
        ];
        assert_eq!(
            ranked("term", items.iter().copied()),
            vec![2, 1],
            "earlier and tighter first"
        );
        assert_eq!(ranked("pl", items.iter().copied())[0], 3);
        assert_eq!(
            ranked("", items.iter().copied()),
            vec![0, 1, 2, 3],
            "all, in order"
        );
    }
}
