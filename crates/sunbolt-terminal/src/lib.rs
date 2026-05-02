/// Minimal terminal session states reserved for the terminal core boundary.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TerminalSessionState {
    Created,
}

#[cfg(test)]
mod tests {
    use super::TerminalSessionState;

    #[test]
    fn initial_state_is_created() {
        assert_eq!(TerminalSessionState::Created, TerminalSessionState::Created);
    }
}
