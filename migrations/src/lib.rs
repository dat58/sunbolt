pub use sea_orm_migration::prelude::*;

mod m20260502_000001_create_users_table;
mod m20260502_000002_create_sessions_table;
mod m20260502_000003_create_terminal_sessions_table;
mod m20260502_000004_create_audit_logs_table;
mod m20260502_000005_create_nodes_table;
mod m20260502_000006_create_node_credentials_table;
mod m20260502_000007_create_node_heartbeats_table;
mod m20260502_000008_create_enrollment_tokens_table;
mod m20260503_000009_create_auth_factors_table;
mod m20260505_000010_create_recovery_codes_table;
mod m20260505_000011_create_passkey_credentials_table;
mod m20260505_000012_create_rbac_tables;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260502_000001_create_users_table::Migration),
            Box::new(m20260502_000002_create_sessions_table::Migration),
            Box::new(m20260502_000003_create_terminal_sessions_table::Migration),
            Box::new(m20260502_000004_create_audit_logs_table::Migration),
            Box::new(m20260502_000005_create_nodes_table::Migration),
            Box::new(m20260502_000006_create_node_credentials_table::Migration),
            Box::new(m20260502_000007_create_node_heartbeats_table::Migration),
            Box::new(m20260502_000008_create_enrollment_tokens_table::Migration),
            Box::new(m20260503_000009_create_auth_factors_table::Migration),
            Box::new(m20260505_000010_create_recovery_codes_table::Migration),
            Box::new(m20260505_000011_create_passkey_credentials_table::Migration),
            Box::new(m20260505_000012_create_rbac_tables::Migration),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{Migrator, MigratorTrait};

    #[test]
    fn migrator_registers_persistent_state_migrations_in_dependency_order() {
        let names: Vec<String> = Migrator::migrations()
            .into_iter()
            .map(|migration| migration.name().to_owned())
            .collect();

        assert_eq!(names.len(), 12);
        assert_eq!(names[0], "m20260502_000001_create_users_table");
        assert_eq!(names[1], "m20260502_000002_create_sessions_table");
        assert_eq!(names[2], "m20260502_000003_create_terminal_sessions_table");
        assert_eq!(names[3], "m20260502_000004_create_audit_logs_table");
        assert_eq!(names[4], "m20260502_000005_create_nodes_table");
        assert_eq!(names[5], "m20260502_000006_create_node_credentials_table");
        assert_eq!(names[6], "m20260502_000007_create_node_heartbeats_table");
        assert_eq!(names[8], "m20260503_000009_create_auth_factors_table");
        assert_eq!(names[11], "m20260505_000012_create_rbac_tables");
    }

    #[test]
    fn migrations_cover_phase_8_3_durable_state_tables() {
        let names: Vec<String> = Migrator::migrations()
            .into_iter()
            .map(|migration| migration.name().to_owned())
            .collect();

        for expected in [
            "users",
            "sessions",
            "auth_factors",
            "rbac",
            "nodes",
            "node_credentials",
            "node_heartbeats",
            "terminal_sessions",
            "audit_logs",
        ] {
            assert!(
                names.iter().any(|name| name.contains(expected)),
                "missing migration covering {expected}"
            );
        }
    }
}
