/// Returns the display title for the web UI shell.
#[must_use]
pub fn app_title() -> String {
    sunbolt_common::product_name().to_owned()
}

#[cfg(test)]
mod tests {
    use super::app_title;

    #[test]
    fn app_title_uses_product_name() {
        assert_eq!(app_title(), "Sunbolt");
    }
}
