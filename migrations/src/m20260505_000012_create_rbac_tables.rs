use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_workspaces(manager).await?;
        create_roles(manager).await?;
        create_permissions(manager).await?;
        create_role_permissions(manager).await?;
        create_workspace_members(manager).await?;
        create_workspace_nodes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkspaceNodes::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WorkspaceMembers::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RolePermissions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Permissions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Roles::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Workspaces::Table).to_owned())
            .await
    }
}

async fn create_workspaces(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Workspaces::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Workspaces::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(Workspaces::Name).string_len(128).not_null())
                .col(
                    ColumnDef::new(Workspaces::CreatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .index(
                    Index::create()
                        .name("idx-workspaces-name")
                        .col(Workspaces::Name)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

async fn create_roles(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Roles::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Roles::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(Roles::Name).string_len(128).not_null())
                .col(
                    ColumnDef::new(Roles::CreatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .index(
                    Index::create()
                        .name("idx-roles-name")
                        .col(Roles::Name)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

async fn create_permissions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Permissions::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Permissions::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Permissions::Permission)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Permissions::CreatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .index(
                    Index::create()
                        .name("idx-permissions-permission")
                        .col(Permissions::Permission)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

async fn create_role_permissions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(RolePermissions::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(RolePermissions::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(RolePermissions::RoleId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(RolePermissions::Permission)
                        .string_len(128)
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-role-permissions-role-id")
                        .from(RolePermissions::Table, RolePermissions::RoleId)
                        .to(Roles::Table, Roles::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .index(
                    Index::create()
                        .name("idx-role-permissions-role-permission")
                        .col(RolePermissions::RoleId)
                        .col(RolePermissions::Permission)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

async fn create_workspace_members(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkspaceMembers::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkspaceMembers::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkspaceMembers::WorkspaceId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkspaceMembers::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkspaceMembers::RoleId)
                        .big_integer()
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-workspace-members-workspace-id")
                        .from(WorkspaceMembers::Table, WorkspaceMembers::WorkspaceId)
                        .to(Workspaces::Table, Workspaces::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-workspace-members-user-id")
                        .from(WorkspaceMembers::Table, WorkspaceMembers::UserId)
                        .to(
                            super::m20260502_000001_create_users_table::Users::Table,
                            super::m20260502_000001_create_users_table::Users::Id,
                        )
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-workspace-members-role-id")
                        .from(WorkspaceMembers::Table, WorkspaceMembers::RoleId)
                        .to(Roles::Table, Roles::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .index(
                    Index::create()
                        .name("idx-workspace-members-user")
                        .col(WorkspaceMembers::WorkspaceId)
                        .col(WorkspaceMembers::UserId)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

async fn create_workspace_nodes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkspaceNodes::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkspaceNodes::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkspaceNodes::WorkspaceId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkspaceNodes::NodeId)
                        .string_len(128)
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk-workspace-nodes-workspace-id")
                        .from(WorkspaceNodes::Table, WorkspaceNodes::WorkspaceId)
                        .to(Workspaces::Table, Workspaces::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .index(
                    Index::create()
                        .name("idx-workspace-nodes-node-id")
                        .col(WorkspaceNodes::NodeId)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    Id,
    Name,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Roles {
    Table,
    Id,
    Name,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Permissions {
    Table,
    Id,
    Permission,
    CreatedAt,
}

#[derive(DeriveIden)]
enum RolePermissions {
    Table,
    Id,
    RoleId,
    Permission,
}

#[derive(DeriveIden)]
enum WorkspaceMembers {
    Table,
    Id,
    WorkspaceId,
    UserId,
    RoleId,
}

#[derive(DeriveIden)]
enum WorkspaceNodes {
    Table,
    Id,
    WorkspaceId,
    NodeId,
}
