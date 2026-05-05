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
        ]
    }
}
