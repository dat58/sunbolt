pub use sea_orm_migration::prelude::*;

mod m20260502_000001_create_users_table;
mod m20260502_000002_create_sessions_table;
mod m20260502_000003_create_terminal_sessions_table;
mod m20260502_000004_create_audit_logs_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260502_000001_create_users_table::Migration),
            Box::new(m20260502_000002_create_sessions_table::Migration),
            Box::new(m20260502_000003_create_terminal_sessions_table::Migration),
            Box::new(m20260502_000004_create_audit_logs_table::Migration),
        ]
    }
}
