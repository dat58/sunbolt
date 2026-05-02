use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuditLogs::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuditLogs::UserId).big_integer())
                    .col(
                        ColumnDef::new(AuditLogs::EventType)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(ColumnDef::new(AuditLogs::TargetType).string_len(64))
                    .col(ColumnDef::new(AuditLogs::TargetId).string_len(128))
                    .col(ColumnDef::new(AuditLogs::MetadataJson).json_binary())
                    .col(ColumnDef::new(AuditLogs::IpAddress).string_len(64))
                    .col(
                        ColumnDef::new(AuditLogs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-audit-logs-user-id")
                            .from(AuditLogs::Table, AuditLogs::UserId)
                            .to(
                                super::m20260502_000001_create_users_table::Users::Table,
                                super::m20260502_000001_create_users_table::Users::Id,
                            )
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .index(
                        Index::create()
                            .name("idx-audit-logs-created-at")
                            .col(AuditLogs::CreatedAt),
                    )
                    .index(
                        Index::create()
                            .name("idx-audit-logs-event-type")
                            .col(AuditLogs::EventType),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLogs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    Id,
    UserId,
    EventType,
    TargetType,
    TargetId,
    MetadataJson,
    IpAddress,
    CreatedAt,
}
