pub(super) fn is_rate_limit_chunk(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("rate limited") || lower.contains("rate limit")
}

#[cfg(test)]
mod tests {
    use super::is_rate_limit_chunk;

    #[test]
    fn detects_rate_limit_variants() {
        assert!(is_rate_limit_chunk("Rate limited. Retrying in 1 seconds."));
        assert!(is_rate_limit_chunk("Rate limit exceeded."));
        assert!(!is_rate_limit_chunk("All good, no limits."));
    }
}
