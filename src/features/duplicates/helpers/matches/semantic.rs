//! AST shape alone confuses unrelated code and repeated authored data.
use crate::shared::models::Function;

pub(super) fn compatible(left: &Function, right: &Function, threshold: f64) -> bool {
    if left.data_only || right.data_only || left.operators != right.operators {
        return false;
    }
    ordered_similarity(&left.semantic, &right.semantic) >= threshold
}

/// Ordered token evidence retains APIs, fields, types, values and local bindings.
/// Two-row LCS bounds memory to the shorter function; shape screening runs first.
fn ordered_similarity(left: &[String], right: &[String]) -> f64 {
    if left.len() < right.len() {
        return ordered_similarity(right, left);
    }
    let total = left.len() + right.len();
    if total == 0 {
        return 0.0;
    }
    let mut previous = vec![0; right.len() + 1];
    let mut current = previous.clone();
    for a in left {
        for (index, b) in right.iter().enumerate() {
            current[index + 1] = if a == b {
                previous[index] + 1
            } else {
                previous[index + 1].max(current[index])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current[0] = 0;
    }
    2.0 * previous[right.len()] as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::ordered_similarity;

    #[test]
    fn statement_order_and_repeated_tokens_are_not_a_bag_of_bigrams() {
        let left = ["open", "read", "close"].map(String::from);
        let right = ["close", "read", "open"].map(String::from);
        assert!(ordered_similarity(&left, &right) < 0.5);
        assert_eq!(ordered_similarity(&left, &left), 1.0);
        assert_eq!(ordered_similarity(&[], &[]), 0.0);
    }
}
