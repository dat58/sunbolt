/// Permission identifiers are resource-oriented strings.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Permission(&'static str);

impl Permission {
    /// Permission required to open a terminal.
    pub const TERMINAL_OPEN: Self = Self("terminal.open");

    /// Returns the stable permission identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Permission;

    #[test]
    fn terminal_open_permission_is_resource_oriented() {
        assert_eq!(Permission::TERMINAL_OPEN.as_str(), "terminal.open");
    }
}
