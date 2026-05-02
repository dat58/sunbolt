use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(NodeHeartbeats::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NodeHeartbeats::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NodeHeartbeats::NodeId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NodeHeartbeats::Status)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NodeHeartbeats::ReceivedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-node-heartbeats-node-id")
                            .from(NodeHeartbeats::Table, NodeHeartbeats::NodeId)
                            .to(
                                super::m20260502_000005_create_nodes_table::Nodes::Table,
                                super::m20260502_000005_create_nodes_table::Nodes::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx-node-heartbeats-node-id")
                            .col(NodeHeartbeats::NodeId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(NodeHeartbeats::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum NodeHeartbeats {
    Table,
    Id,
    NodeId,
    Status,
    ReceivedAt,
}
