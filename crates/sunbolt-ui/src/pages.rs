use dioxus::prelude::*;

use crate::components::{
    button_class, form, layout, status_badge_class, table_list, ButtonVariant, StatusTone,
};
use crate::terminal_workspace::{
    TERMINAL_AUTH_PANEL_ID, TERMINAL_DETACHED_SESSIONS_ID, TERMINAL_ERROR_ID, TERMINAL_MOUNT_ID,
    TERMINAL_NODE_INPUT_ID, TERMINAL_STATUS_ID, TERMINAL_TABS_ID,
};
use crate::TERMINAL_WS_ENDPOINT;

/// First local terminal page.
#[component]
pub fn TerminalPageBody() -> Element {
    rsx! {
        section {
            class: "sunbolt-terminal-page",
            div {
                class: "sunbolt-terminal-toolbar",
                div {
                    class: "min-w-0",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Terminal Workspace"
                    }
                    p {
                        class: "m-0 text-xs text-terminal-muted",
                        "Authenticated local shell with audit-ready session controls."
                    }
                }
                div {
                    class: "sunbolt-terminal-controls",
                    input {
                        id: TERMINAL_NODE_INPUT_ID,
                        class: form::TEXT_INPUT,
                        placeholder: "node id, empty for local",
                        value: ""
                    }
                    div {
                        id: TERMINAL_STATUS_ID,
                        class: status_badge_class(StatusTone::Connecting),
                        "Connecting"
                    }
                    button {
                        id: "sunbolt-terminal-mfa",
                        class: button_class(ButtonVariant::Primary),
                        "Step-up MFA"
                    }
                    button {
                        id: "sunbolt-terminal-new",
                        class: button_class(ButtonVariant::Secondary),
                        "New"
                    }
                    button {
                        id: "sunbolt-terminal-detach",
                        class: button_class(ButtonVariant::Secondary),
                        "Detach"
                    }
                    button {
                        id: "sunbolt-terminal-close-tab",
                        class: button_class(ButtonVariant::Secondary),
                        "Close Tab"
                    }
                    button {
                        id: "sunbolt-terminal-reconnect",
                        class: button_class(ButtonVariant::Secondary),
                        disabled: true,
                        "Reconnect"
                    }
                    button {
                        id: "sunbolt-terminal-retry",
                        class: button_class(ButtonVariant::Secondary),
                        "Retry"
                    }
                    button {
                        id: "sunbolt-terminal-close",
                        class: button_class(ButtonVariant::Danger),
                        "Terminate"
                    }
                }
            }
            div {
                id: TERMINAL_TABS_ID,
                class: "sunbolt-terminal-tabs",
                role: "tablist"
            }
            div {
                id: TERMINAL_DETACHED_SESSIONS_ID,
                class: "sunbolt-detached-sessions"
            }
            div {
                id: TERMINAL_ERROR_ID,
                class: "sunbolt-alert hidden items-center",
                role: "status"
            }
            div {
                id: TERMINAL_AUTH_PANEL_ID,
                class: "sunbolt-auth-panel hidden",
                div {
                    class: "min-w-0",
                    p {
                        class: "m-0 text-sm font-semibold text-terminal-text",
                        "Sign in to open a terminal"
                    }
                    p {
                        class: "m-0 text-xs text-terminal-muted",
                        "Use the local bootstrap admin or another account already created in the control plane."
                    }
                }
                input {
                    id: "sunbolt-terminal-email",
                    class: form::TEXT_INPUT,
                    placeholder: "email",
                    autocomplete: "username"
                }
                input {
                    id: "sunbolt-terminal-password",
                    class: form::TEXT_INPUT,
                    placeholder: "password",
                    r#type: "password",
                    autocomplete: "current-password"
                }
                button {
                    id: "sunbolt-terminal-login",
                    class: button_class(ButtonVariant::Primary),
                    "Sign In"
                }
            }
            div {
                id: TERMINAL_MOUNT_ID,
                class: "sunbolt-terminal-body",
                tabindex: "0",
                role: "application",
                "aria-label": "Terminal viewport",
                "data-ws-endpoint": TERMINAL_WS_ENDPOINT,
                "Terminal loading"
            }
        }
    }
}

#[component]
pub fn AccessHistoryPage() -> Element {
    rsx! {
        section {
            class: layout::PAGE,
            div {
                class: layout::CARD,
                h2 {
                    class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "Access History"
                }
                div {
                    class: table_list::TABLE_WRAP,
                    table {
                        class: table_list::TABLE,
                        thead {
                            tr {
                                th { "Time" }
                                th { "Event" }
                                th { "Actor" }
                                th { "Message" }
                            }
                        }
                        tbody {
                            tr {
                                td { class: "text-terminal-muted", "Pending" }
                                td { class: "text-terminal-text", "user.login.success" }
                                td { class: "text-terminal-text", "admin@example.com" }
                                td { class: "text-terminal-muted", "Awaiting backend list wiring" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn AuditLogPage() -> Element {
    rsx! {
        section {
            class: layout::PAGE,
            div {
                class: layout::CARD,
                h2 {
                    class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "Audit Logs"
                }
                div {
                    class: table_list::TABLE_WRAP,
                    table {
                        class: table_list::TABLE,
                        thead {
                            tr {
                                th { "Time" }
                                th { "Kind" }
                                th { "Actor" }
                                th { "Message" }
                            }
                        }
                        tbody {
                            tr {
                                td { class: "text-terminal-muted", "Pending" }
                                td { class: "text-terminal-text", "terminal.opened" }
                                td { class: "text-terminal-text", "admin@example.com" }
                                td { class: "text-terminal-muted", "Awaiting backend list wiring" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn NodesPage() -> Element {
    rsx! {
        section {
            class: "sunbolt-page grid gap-4",
            div {
                class: layout::CARD,
                h2 {
                    class: "mb-2 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "Node Enrollment"
                }
                p {
                    class: "mb-3 mt-0 text-sm text-terminal-muted",
                    "Start an enrolled agent with a one-time token from the control plane."
                }
                pre {
                    class: "sunbolt-command",
                    "SUNBOLT_CONTROL_PLANE_URL=http://127.0.0.1:3000 SUNBOLT_AGENT_ENROLLMENT_TOKEN=<token> cargo run -p sunbolt-agent"
                }
            }
            div {
                class: table_list::TABLE_WRAP,
                table {
                    class: table_list::TABLE,
                    thead {
                        tr {
                            th { "Node" }
                            th { "Hostname" }
                            th { "OS" }
                            th { "Status" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        tr {
                            td { class: "font-mono text-xs text-terminal-text", "node-1" }
                            td { class: "text-terminal-text", "host-a" }
                            td { class: "text-terminal-text", "linux" }
                            td {
                                span { class: status_badge_class(StatusTone::Connected), "online" }
                            }
                            td {
                                class: "flex gap-2",
                                button { class: button_class(ButtonVariant::Secondary), "Details" }
                                button { class: button_class(ButtonVariant::Danger), "Revoke" }
                            }
                        }
                    }
                }
            }
            div {
                class: layout::CARD,
                h3 {
                    class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "Node Details"
                }
                dl {
                    class: "grid grid-cols-[120px_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm",
                    dt { class: "text-terminal-muted", "Node" }
                    dd { class: "m-0 text-terminal-text", "node-1" }
                    dt { class: "text-terminal-muted", "Agent" }
                    dd { class: "m-0 text-terminal-text", "0.1.0" }
                    dt { class: "text-terminal-muted", "Architecture" }
                    dd { class: "m-0 text-terminal-text", "x86_64" }
                    dt { class: "text-terminal-muted", "Remote Terminal" }
                    dd { class: "m-0 text-terminal-muted", "Enter the node id in the terminal toolbar to route through the agent." }
                }
            }
        }
    }
}

#[component]
pub fn SecurityPage() -> Element {
    rsx! {
        section {
            class: "sunbolt-page grid gap-4",
            div {
                class: layout::CARD,
                div {
                    class: "mb-3 flex items-center justify-between gap-3",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Passkeys"
                    }
                    button {
                        class: button_class(ButtonVariant::Primary),
                        "Add passkey"
                    }
                }
                div {
                    class: table_list::TABLE_WRAP,
                    table {
                        class: table_list::TABLE,
                        thead {
                            tr {
                                th { "Label" }
                                th { "Credential" }
                                th { "Status" }
                                th { "Actions" }
                            }
                        }
                        tbody {
                            tr {
                                td { class: "text-terminal-text", "Laptop passkey" }
                                td { class: "font-mono text-xs text-terminal-muted", "credential-1" }
                                td {
                                    span { class: status_badge_class(StatusTone::Connected), "enabled" }
                                }
                                td {
                                    button { class: button_class(ButtonVariant::Danger), "Disable" }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: layout::CARD,
                h3 {
                    class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "WebAuthn"
                }
                dl {
                    class: "grid grid-cols-[160px_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm",
                    dt { class: "text-terminal-muted", "Backend crate" }
                    dd { class: "m-0 text-terminal-text", "webauthn-rs" }
                    dt { class: "text-terminal-muted", "Registration" }
                    dd { class: "m-0 text-terminal-muted", "Challenge API ready" }
                    dt { class: "text-terminal-muted", "Authentication" }
                    dd { class: "m-0 text-terminal-muted", "Challenge API ready" }
                }
            }
        }
    }
}

#[component]
pub fn AdminPage() -> Element {
    rsx! {
        section {
            class: "sunbolt-page grid gap-4",
            div {
                class: "grid grid-cols-1 gap-4 lg:grid-cols-2",
                AdminTable {
                    title: "Workspaces",
                    headers: vec!["Name", "Nodes", "Members"],
                    rows: vec![vec!["Operations", "node-1", "admin@example.com"]],
                }
                AdminTable {
                    title: "Roles",
                    headers: vec!["Role", "Permissions", "Members"],
                    rows: vec![vec!["Operator", "terminal.open, node.view", "1"]],
                }
            }
            div {
                class: layout::CARD,
                div {
                    class: "mb-3 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Workspace Access"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        button { class: button_class(ButtonVariant::Primary), "Add member" }
                        button { class: button_class(ButtonVariant::Secondary), "Grant role" }
                    }
                }
                div {
                    class: table_list::TABLE_WRAP,
                    table {
                        class: table_list::TABLE,
                        thead {
                            tr {
                                th { "Workspace" }
                                th { "User" }
                                th { "Role" }
                                th { "Actions" }
                            }
                        }
                        tbody {
                            tr {
                                td { class: "text-terminal-text", "Operations" }
                                td { class: "text-terminal-text", "admin@example.com" }
                                td { class: "text-terminal-text", "Admin" }
                                td {
                                    button { class: button_class(ButtonVariant::Danger), "Remove" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AdminTable(
    title: &'static str,
    headers: Vec<&'static str>,
    rows: Vec<Vec<&'static str>>,
) -> Element {
    rsx! {
        div {
            class: layout::CARD,
            h2 {
                class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                "{title}"
            }
            div {
                class: table_list::TABLE_WRAP,
                table {
                    class: table_list::TABLE,
                    thead {
                        tr {
                            for header in headers {
                                th { "{header}" }
                            }
                        }
                    }
                    tbody {
                        for row in rows {
                            tr {
                                for cell in row {
                                    td { class: "text-terminal-text", "{cell}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
