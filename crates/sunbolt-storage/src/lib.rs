/// Placeholder storage backend marker.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StorageBackend {
    Postgres,
}

#[cfg(test)]
mod tests {
    use super::StorageBackend;

    #[test]
    fn postgres_is_the_initial_storage_direction() {
        assert_eq!(StorageBackend::Postgres, StorageBackend::Postgres);
    }
}
