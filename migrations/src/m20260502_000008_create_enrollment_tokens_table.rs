use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EnrollmentTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EnrollmentTokens::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EnrollmentTokens::TokenHash)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(ColumnDef::new(EnrollmentTokens::CreatedByUserId).big_integer())
                    .col(ColumnDef::new(EnrollmentTokens::UsedByNodeId).big_integer())
                    .col(ColumnDef::new(EnrollmentTokens::UsedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(EnrollmentTokens::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EnrollmentTokens::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-enrollment-tokens-created-by-user-id")
                            .from(EnrollmentTokens::Table, EnrollmentTokens::CreatedByUserId)
                            .to(
                                super::m20260502_000001_create_users_table::Users::Table,
                                super::m20260502_000001_create_users_table::Users::Id,
                            )
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-enrollment-tokens-used-by-node-id")
                            .from(EnrollmentTokens::Table, EnrollmentTokens::UsedByNodeId)
                            .to(
                                super::m20260502_000005_create_nodes_table::Nodes::Table,
                                super::m20260502_000005_create_nodes_table::Nodes::Id,
                            )
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .index(
                        Index::create()
                            .name("idx-enrollment-tokens-token-hash-unique")
                            .col(EnrollmentTokens::TokenHash)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EnrollmentTokens::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum EnrollmentTokens {
    Table,
    Id,
    TokenHash,
    CreatedByUserId,
    UsedByNodeId,
    UsedAt,
    ExpiresAt,
    CreatedAt,
}
