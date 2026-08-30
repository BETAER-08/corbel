pub const DEFAULT_IMPACT_TOKEN_BUDGET: usize = 8000;
pub const DEFAULT_FIND_TOKEN_BUDGET: usize = 8000;

pub struct TokenBudget {
    limit: usize,
    used: usize,
}

impl TokenBudget {
    pub fn new(limit: usize) -> Self {
        TokenBudget { limit, used: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.used)
    }

    pub fn try_consume(&mut self, text_len_estimate: usize) -> bool {
        if text_len_estimate > self.remaining() {
            return false;
        }
        self.used += text_len_estimate;
        true
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 3).max(1)
}

pub fn estimate_node_tokens(name: &str, file: &str, line: u32, resolution: &str) -> usize {
    let json_like = format!(
        "{{\"name\":\"{name}\",\"file\":\"{file}\",\"line\":{line},\"resolution\":\"{resolution}\"}}"
    );
    estimate_tokens(&json_like)
}

pub fn estimate_symbol_tokens(
    name: &str,
    file: &str,
    line: u32,
    kind: &str,
    signature: Option<&str>,
    is_public: bool,
) -> usize {
    let signature_text = signature.unwrap_or("");
    let json_like = format!(
        "{{\"name\":\"{name}\",\"file\":\"{file}\",\"line\":{line},\"kind\":\"{kind}\",\"signature\":\"{signature_text}\",\"is_public\":{is_public}}}"
    );
    estimate_tokens(&json_like)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_consume_within_budget_succeeds_and_updates_used() {
        let mut budget = TokenBudget::new(100);
        assert!(budget.try_consume(40));
        assert_eq!(budget.remaining(), 60);
    }

    #[test]
    fn try_consume_over_budget_fails_and_leaves_used_unchanged() {
        let mut budget = TokenBudget::new(100);
        assert!(budget.try_consume(60));
        assert!(!budget.try_consume(50));
        assert_eq!(budget.remaining(), 40);
    }

    #[test]
    fn estimate_tokens_of_empty_string_is_at_least_one() {
        assert_eq!(estimate_tokens(""), 1);
    }

    #[test]
    fn estimate_tokens_scales_with_length() {
        let short = estimate_tokens("abc");
        let long = estimate_tokens(&"a".repeat(300));
        assert!(long > short);
        assert_eq!(long, 100);
    }

    #[test]
    fn remaining_and_is_exhausted_are_accurate() {
        let mut budget = TokenBudget::new(10);
        assert_eq!(budget.remaining(), 10);
        assert!(!budget.is_exhausted());

        assert!(budget.try_consume(10));
        assert_eq!(budget.remaining(), 0);
        assert!(budget.is_exhausted());

        assert!(!budget.try_consume(1));
    }

    #[test]
    fn estimate_node_tokens_reflects_field_lengths() {
        let small = estimate_node_tokens("a", "a.rs", 1, "same-file");
        let large = estimate_node_tokens(
            "a_much_longer_symbol_name_here",
            "some/deeply/nested/module/path/file.rs",
            123,
            "global-unique",
        );
        assert!(large > small);
    }
}
