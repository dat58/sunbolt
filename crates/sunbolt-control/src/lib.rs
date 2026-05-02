/// Returns a stable name for the control plane component.
#[must_use]
pub fn component_name() -> String {
    format!("{} control plane", sunbolt_common::product_name())
}

#[cfg(test)]
mod tests {
    use super::component_name;

    #[test]
    fn component_name_mentions_control_plane() {
        assert_eq!(component_name(), "Sunbolt control plane");
    }
}
