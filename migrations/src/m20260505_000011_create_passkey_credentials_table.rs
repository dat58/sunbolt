use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PasskeyCredentials::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PasskeyCredentials::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(PasskeyCredentials::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PasskeyCredentials::CredentialId)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PasskeyCredentials::PublicKey)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PasskeyCredentials::Label)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PasskeyCredentials::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(PasskeyCredentials::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-passkey-credentials-user-id")
                            .from(PasskeyCredentials::Table, PasskeyCredentials::UserId)
                            .to(
                                super::m20260502_000001_create_users_table::Users::Table,
                                super::m20260502_000001_create_users_table::Users::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx-passkey-credentials-credential-id")
                            .col(PasskeyCredentials::CredentialId)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PasskeyCredentials::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PasskeyCredentials {
    Table,
    Id,
    UserId,
    CredentialId,
    PublicKey,
    Label,
    Enabled,
    CreatedAt,
}
