use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RecoveryCodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RecoveryCodes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RecoveryCodes::FactorId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RecoveryCodes::CodeHash)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(ColumnDef::new(RecoveryCodes::UsedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(RecoveryCodes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-recovery-codes-factor-id")
                            .from(RecoveryCodes::Table, RecoveryCodes::FactorId)
                            .to(
                                super::m20260503_000009_create_auth_factors_table::AuthFactors::Table,
                                super::m20260503_000009_create_auth_factors_table::AuthFactors::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx-recovery-codes-factor-hash")
                            .col(RecoveryCodes::FactorId)
                            .col(RecoveryCodes::CodeHash)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RecoveryCodes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum RecoveryCodes {
    Table,
    Id,
    FactorId,
    CodeHash,
    UsedAt,
    CreatedAt,
}
