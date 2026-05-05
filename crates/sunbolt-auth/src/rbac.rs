use serde::{Deserialize, Serialize};

/// Workspace groups nodes, users, and permissions.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
}

/// Role assigned to a user inside a workspace.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: u64,
    pub name: String,
}

/// User membership in a workspace.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub workspace_id: u64,
    pub user_id: u64,
    pub role_id: u64,
}

/// Permission granted to a role.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RolePermission {
    pub role_id: u64,
    pub permission: String,
}

/// Node mapped to a workspace for workspace-scoped checks.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceNode {
    pub workspace_id: u64,
    pub node_id: String,
}

#[must_use]
pub(crate) fn workspace(id: u64, name: &str) -> Workspace {
    Workspace {
        id,
        name: name.trim().to_owned(),
    }
}

#[must_use]
pub(crate) fn role(id: u64, name: &str) -> Role {
    Role {
        id,
        name: name.trim().to_owned(),
    }
}

#[must_use]
pub(crate) const fn workspace_member(
    workspace_id: u64,
    user_id: u64,
    role_id: u64,
) -> WorkspaceMember {
    WorkspaceMember {
        workspace_id,
        user_id,
        role_id,
    }
}

#[must_use]
pub(crate) fn role_permission(role_id: u64, permission: &str) -> RolePermission {
    RolePermission {
        role_id,
        permission: permission.to_owned(),
    }
}

#[must_use]
pub(crate) fn workspace_node(workspace_id: u64, node_id: &str) -> WorkspaceNode {
    WorkspaceNode {
        workspace_id,
        node_id: node_id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{role, role_permission, workspace, workspace_member, workspace_node};

    #[test]
    fn rbac_records_are_constructed_from_inputs() {
        assert_eq!(workspace(1, " Operations ").name, "Operations");
        assert_eq!(role(2, " Operator ").name, "Operator");
        assert_eq!(workspace_member(1, 7, 2).user_id, 7);
        assert_eq!(
            role_permission(2, "terminal.open").permission,
            "terminal.open"
        );
        assert_eq!(workspace_node(1, "node-1").node_id, "node-1");
    }
}
