use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuthFactors::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuthFactors::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuthFactors::UserId).big_integer().not_null())
                    .col(
                        ColumnDef::new(AuthFactors::FactorType)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AuthFactors::Label)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AuthFactors::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(AuthFactors::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AuthFactors::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-auth-factors-user-id")
                            .from(AuthFactors::Table, AuthFactors::UserId)
                            .to(
                                super::m20260502_000001_create_users_table::Users::Table,
                                super::m20260502_000001_create_users_table::Users::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx-auth-factors-user-type")
                            .col(AuthFactors::UserId)
                            .col(AuthFactors::FactorType),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuthFactors::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum AuthFactors {
    Table,
    Id,
    UserId,
    FactorType,
    Label,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
