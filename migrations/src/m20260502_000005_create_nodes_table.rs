use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Nodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Nodes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Nodes::NodeId).string_len(128).not_null())
                    .col(
                        ColumnDef::new(Nodes::DisplayName)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Nodes::Hostname).string_len(255).not_null())
                    .col(ColumnDef::new(Nodes::Os).string_len(64).not_null())
                    .col(
                        ColumnDef::new(Nodes::Architecture)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Nodes::AgentVersion)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Nodes::Status).string_len(32).not_null())
                    .col(
                        ColumnDef::new(Nodes::EnrolledAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("idx-nodes-node-id-unique")
                            .col(Nodes::NodeId)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Nodes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Nodes {
    Table,
    Id,
    NodeId,
    DisplayName,
    Hostname,
    Os,
    Architecture,
    AgentVersion,
    Status,
    EnrolledAt,
}
