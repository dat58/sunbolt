use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TerminalSessions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TerminalSessions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TerminalSessions::SessionId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TerminalSessions::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(TerminalSessions::NodeId).string_len(128))
                    .col(
                        ColumnDef::new(TerminalSessions::State)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(TerminalSessions::StartedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(TerminalSessions::EndedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(TerminalSessions::ExitCode).integer())
                    .col(
                        ColumnDef::new(TerminalSessions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-terminal-sessions-user-id")
                            .from(TerminalSessions::Table, TerminalSessions::UserId)
                            .to(
                                super::m20260502_000001_create_users_table::Users::Table,
                                super::m20260502_000001_create_users_table::Users::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx-terminal-sessions-session-id-unique")
                            .col(TerminalSessions::SessionId)
                            .unique(),
                    )
                    .index(
                        Index::create()
                            .name("idx-terminal-sessions-user-id")
                            .col(TerminalSessions::UserId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TerminalSessions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum TerminalSessions {
    Table,
    Id,
    SessionId,
    UserId,
    NodeId,
    State,
    StartedAt,
    EndedAt,
    ExitCode,
    CreatedAt,
}
