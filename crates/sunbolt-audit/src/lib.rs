/// Stable audit event names reserved by the audit boundary.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AuditEventKind {
    TerminalOpened,
}

impl AuditEventKind {
    /// Returns the stable event name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TerminalOpened => "terminal.opened",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AuditEventKind;

    #[test]
    fn terminal_opened_event_name_is_stable() {
        assert_eq!(AuditEventKind::TerminalOpened.as_str(), "terminal.opened");
    }
}
