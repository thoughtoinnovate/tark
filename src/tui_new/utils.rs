//! Utility functions for TUI

/// Trait for types that can be filtered by text
pub trait Filterable {
    /// Get the text to use for filtering
    fn filter_text(&self) -> &str;
}

/// Filter items by text, case-insensitive
///
/// Returns all items if filter is empty, otherwise returns items
/// whose filter_text() contains the filter string (case-insensitive).
pub fn filter_items<T: Filterable>(items: Vec<T>, filter: &str) -> Vec<T> {
    if filter.is_empty() {
        items
    } else {
        let filter_lower = filter.to_lowercase();
        items
            .into_iter()
            .filter(|item| item.filter_text().to_lowercase().contains(&filter_lower))
            .collect()
    }
}

// Implement Filterable for common ui_backend types
impl Filterable for crate::ui_backend::ProviderInfo {
    fn filter_text(&self) -> &str {
        &self.name
    }
}

impl Filterable for crate::ui_backend::ModelInfo {
    fn filter_text(&self) -> &str {
        &self.name
    }
}

impl Filterable for crate::ui_backend::ThemePreset {
    fn filter_text(&self) -> &str {
        self.display_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestItem {
        name: String,
    }

    impl Filterable for TestItem {
        fn filter_text(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_filter_empty() {
        let items = vec![
            TestItem {
                name: "foo".to_string(),
            },
            TestItem {
                name: "bar".to_string(),
            },
        ];
        let filtered = filter_items(items.clone(), "");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_match() {
        let items = vec![
            TestItem {
                name: "foo bar".to_string(),
            },
            TestItem {
                name: "baz qux".to_string(),
            },
        ];
        let filtered = filter_items(items, "bar");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "foo bar");
    }

    #[test]
    fn test_filter_case_insensitive() {
        let items = vec![
            TestItem {
                name: "FooBar".to_string(),
            },
            TestItem {
                name: "bazqux".to_string(),
            },
        ];
        let filtered = filter_items(items, "FOO");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "FooBar");
    }
}
