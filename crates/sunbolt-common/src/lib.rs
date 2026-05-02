/// Returns the product name used across the workspace.
#[must_use]
pub const fn product_name() -> &'static str {
    "Sunbolt"
}

#[cfg(test)]
mod tests {
    use super::product_name;

    #[test]
    fn product_name_is_sunbolt() {
        assert_eq!(product_name(), "Sunbolt");
    }
}
