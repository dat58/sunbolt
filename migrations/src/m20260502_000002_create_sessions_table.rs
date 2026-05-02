use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Sessions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Sessions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Sessions::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Sessions::TokenHash).string().not_null())
                    .col(ColumnDef::new(Sessions::IpAddress).string_len(64))
                    .col(ColumnDef::new(Sessions::UserAgent).string_len(512))
                    .col(ColumnDef::new(Sessions::LastSeenAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Sessions::ExpiresAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Sessions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-sessions-user-id")
                            .from(Sessions::Table, Sessions::UserId)
                            .to(
                                super::m20260502_000001_create_users_table::Users::Table,
                                super::m20260502_000001_create_users_table::Users::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx-sessions-token-hash-unique")
                            .col(Sessions::TokenHash)
                            .unique(),
                    )
                    .index(
                        Index::create()
                            .name("idx-sessions-user-id")
                            .col(Sessions::UserId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Sessions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Sessions {
    Table,
    Id,
    UserId,
    TokenHash,
    IpAddress,
    UserAgent,
    LastSeenAt,
    ExpiresAt,
    CreatedAt,
}
