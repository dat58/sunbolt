use dioxus::prelude::*;

use crate::components::{
    button_class, form, layout, status_badge_class, table_list, ButtonVariant, StatusTone,
};
use crate::terminal_workspace::{
    TERMINAL_AUTH_PANEL_ID, TERMINAL_DETACHED_SESSIONS_ID, TERMINAL_ERROR_ID, TERMINAL_MOUNT_ID,
    TERMINAL_NODE_INPUT_ID, TERMINAL_STATUS_ID, TERMINAL_TABS_ID,
};
use crate::TERMINAL_WS_ENDPOINT;

#[component]
pub fn DashboardPage() -> Element {
    rsx! {
        section {
            class: layout::PAGE,
            div {
                class: layout::PAGE_HEADER,
                div {
                    class: "min-w-0",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Production Dashboard"
                    }
                    p {
                        class: "m-0 text-sm text-terminal-muted",
                        "Dense control-plane overview for terminal operations, agent health, and security review."
                    }
                }
                div {
                    class: table_list::TOOLBAR_ACTIONS,
                    button { class: button_class(ButtonVariant::Primary), "Open Terminal" }
                    button { class: button_class(ButtonVariant::Secondary), "Review Audit" }
                }
            }
            div {
                class: layout::DASHBOARD_GRID,
                MetricCard {
                    label: "Active Sessions",
                    value: "3",
                    detail: "2 local, 1 remote"
                }
                MetricCard {
                    label: "Connected Nodes",
                    value: "1",
                    detail: "1 degraded transport"
                }
                MetricCard {
                    label: "Detached Sessions",
                    value: "2",
                    detail: "Reattach window active"
                }
                MetricCard {
                    label: "Audit Events",
                    value: "128",
                    detail: "Last 24 hours"
                }
            }
            div {
                class: layout::DASHBOARD_MAIN,
                div {
                    class: layout::CARD,
                    div {
                        class: table_list::TOOLBAR,
                        div {
                            h3 {
                                class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                                "Terminal Queue"
                            }
                            p {
                                class: "m-0 text-xs text-terminal-muted",
                                "Open, detached, and degraded sessions."
                            }
                        }
                        div {
                            class: table_list::TOOLBAR_ACTIONS,
                            input {
                                class: form::TEXT_INPUT,
                                r#type: "search",
                                placeholder: "Search sessions"
                            }
                            select {
                                class: form::TEXT_INPUT,
                                "aria-label": "Session state filter",
                                option { "All states" }
                                option { "Active" }
                                option { "Detached" }
                                option { "Degraded" }
                            }
                        }
                    }
                    div {
                        class: table_list::MOBILE_LIST,
                        MobileRecord {
                            eyebrow: "active",
                            title: "term-84f2a9",
                            detail: "local - admin@example.com - just now"
                        }
                        MobileRecord {
                            eyebrow: "degraded",
                            title: "term-d19344",
                            detail: "node-1 - admin@example.com - 2 min ago"
                        }
                    }
                    div {
                        class: table_list::TABLE_WRAP,
                        table {
                            class: table_list::TABLE,
                            thead {
                                tr {
                                    th { "Session" }
                                    th { "Node" }
                                    th { "State" }
                                    th { "Owner" }
                                    th { "Updated" }
                                }
                            }
                            tbody {
                                tr {
                                    td { class: "font-mono text-xs text-terminal-text", "term-84f2a9" }
                                    td { class: "text-terminal-text", "local" }
                                    td {
                                        span { class: status_badge_class(StatusTone::Connected), "active" }
                                    }
                                    td { class: "text-terminal-text", "admin@example.com" }
                                    td { class: "text-terminal-muted", "just now" }
                                }
                                tr {
                                    td { class: "font-mono text-xs text-terminal-text", "term-d19344" }
                                    td { class: "text-terminal-text", "node-1" }
                                    td {
                                        span { class: status_badge_class(StatusTone::Degraded), "degraded" }
                                    }
                                    td { class: "text-terminal-text", "admin@example.com" }
                                    td { class: "text-terminal-muted", "2 min ago" }
                                }
                            }
                        }
                    }
                }
                div {
                    class: layout::CARD,
                    div {
                        class: table_list::TOOLBAR,
                        div {
                            h3 {
                                class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                                "Security Review"
                            }
                            p {
                                class: "m-0 text-xs text-terminal-muted",
                                "Recent high-signal events."
                            }
                        }
                        button { class: button_class(ButtonVariant::Secondary), "View all" }
                    }
                    div {
                        class: table_list::DENSE_LIST,
                        DashboardEvent {
                            event: "terminal.opened",
                            actor: "admin@example.com",
                            detail: "local shell"
                        }
                        DashboardEvent {
                            event: "agent.transport.negotiated",
                            actor: "node-1",
                            detail: "WebSocket TCP/443"
                        }
                        DashboardEvent {
                            event: "terminal.detached",
                            actor: "admin@example.com",
                            detail: "term-d19344"
                        }
                    }
                }
            }
        }
    }
}

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
                    details {
                        class: "sunbolt-node-selector-sheet",
                        summary {
                            class: button_class(ButtonVariant::Secondary),
                            "Node"
                        }
                        div {
                            class: "sunbolt-bottom-sheet-panel",
                            label {
                                class: "sunbolt-sheet-label",
                                "Target node"
                            }
                            input {
                                id: TERMINAL_NODE_INPUT_ID,
                                class: form::TEXT_INPUT,
                                placeholder: "node id, empty for local",
                                value: ""
                            }
                        }
                    }
                    div {
                        id: TERMINAL_STATUS_ID,
                        class: status_badge_class(StatusTone::Connecting),
                        "Connecting"
                    }
                    details {
                        class: "sunbolt-session-actions-sheet",
                        summary {
                            class: button_class(ButtonVariant::Primary),
                            "Actions"
                        }
                        div {
                            class: "sunbolt-bottom-sheet-panel",
                            div {
                                class: "sunbolt-bottom-sheet-actions",
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
                    }
                }
            }
            div {
                class: "sunbolt-session-switcher",
                div {
                    id: TERMINAL_TABS_ID,
                    class: "sunbolt-terminal-tabs",
                    role: "tablist"
                }
                div {
                    id: TERMINAL_DETACHED_SESSIONS_ID,
                    class: "sunbolt-detached-sessions"
                }
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
                class: "sunbolt-terminal-workspace-grid",
                aside {
                    class: "sunbolt-tablet-node-pane",
                    div {
                        class: "sunbolt-tablet-node-list",
                        button { class: "sunbolt-tablet-node-row sunbolt-tablet-node-row-active", "local" }
                        button { class: "sunbolt-tablet-node-row", "node-1" }
                    }
                    div {
                        class: "sunbolt-tablet-node-detail",
                        p { class: "m-0 text-xs font-semibold text-terminal-text", "Selected node" }
                        p { class: "m-0 text-xs text-terminal-muted", "Use the terminal pane to connect, detach, reattach, or terminate." }
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
            div {
                class: "sunbolt-mobile-accessory-toolbar",
                "aria-label": "Mobile terminal accessory controls",
                button { id: "sunbolt-mobile-key-ctrl", r#type: "button", "Ctrl" }
                button { id: "sunbolt-mobile-key-esc", r#type: "button", "Esc" }
                button { id: "sunbolt-mobile-key-tab", r#type: "button", "Tab" }
                button { id: "sunbolt-mobile-key-up", r#type: "button", "Up" }
                button { id: "sunbolt-mobile-key-left", r#type: "button", "Left" }
                button { id: "sunbolt-mobile-key-down", r#type: "button", "Down" }
                button { id: "sunbolt-mobile-key-right", r#type: "button", "Right" }
                button { id: "sunbolt-mobile-key-paste", r#type: "button", "Paste" }
                button { id: "sunbolt-mobile-action-reconnect", r#type: "button", "Reconnect" }
                button { id: "sunbolt-mobile-action-detach", r#type: "button", "Detach" }
                button { id: "sunbolt-mobile-action-terminate", r#type: "button", "Terminate" }
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
                class: layout::PAGE_HEADER,
                div {
                    class: "min-w-0",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Access History"
                    }
                    p {
                        class: "m-0 text-sm text-terminal-muted",
                        "Authentication, MFA, and terminal access events optimized for desktop review."
                    }
                }
                div {
                    class: table_list::TOOLBAR_ACTIONS,
                    input {
                        class: form::TEXT_INPUT,
                        r#type: "search",
                        placeholder: "Search actor or event"
                    }
                    select {
                        class: form::TEXT_INPUT,
                        "aria-label": "Access history filter",
                        option { "All access events" }
                        option { "Login events" }
                        option { "MFA events" }
                        option { "Terminal events" }
                    }
                }
            }
            div {
                class: layout::CARD,
                div {
                    class: table_list::MOBILE_LIST,
                    MobileRecord {
                        eyebrow: "user.login.success",
                        title: "admin@example.com",
                        detail: "Pending - Awaiting backend list wiring"
                    }
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
                TablePagination { label: "1-1 of 1 access records" }
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
                class: layout::PAGE_HEADER,
                div {
                    class: "min-w-0",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Audit Logs"
                    }
                    p {
                        class: "m-0 text-sm text-terminal-muted",
                        "Append-only security events with fast desktop filtering."
                    }
                }
                div {
                    class: table_list::TOOLBAR_ACTIONS,
                    input {
                        class: form::TEXT_INPUT,
                        r#type: "search",
                        placeholder: "Search audit logs"
                    }
                    select {
                        class: form::TEXT_INPUT,
                        "aria-label": "Audit kind filter",
                        option { "All audit kinds" }
                        option { "Terminal" }
                        option { "Agent" }
                        option { "Node" }
                        option { "Permission" }
                    }
                }
            }
            div {
                class: layout::CARD,
                div {
                    class: table_list::MOBILE_LIST,
                    MobileRecord {
                        eyebrow: "terminal.opened",
                        title: "admin@example.com",
                        detail: "Pending - Awaiting backend list wiring"
                    }
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
                TablePagination { label: "1-1 of 1 audit records" }
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
                class: layout::PAGE_HEADER,
                div {
                    class: "min-w-0",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Node Management"
                    }
                    p {
                        class: "m-0 text-sm text-terminal-muted",
                        "Search, filter, inspect, and revoke managed agent nodes."
                    }
                }
                div {
                    class: table_list::TOOLBAR_ACTIONS,
                    input {
                        class: form::TEXT_INPUT,
                        r#type: "search",
                        placeholder: "Search nodes"
                    }
                    select {
                        class: form::TEXT_INPUT,
                        "aria-label": "Node status filter",
                        option { "All statuses" }
                        option { "Online" }
                        option { "Degraded" }
                        option { "Offline" }
                        option { "Revoked" }
                    }
                    button { class: button_class(ButtonVariant::Primary), "Enroll node" }
                }
            }
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
                class: layout::CARD,
                div {
                    class: table_list::TOOLBAR,
                    div {
                        h3 {
                            class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                            "Managed Nodes"
                        }
                        p {
                            class: "m-0 text-xs text-terminal-muted",
                            "Desktop table with search and status filters."
                        }
                    }
                    div {
                        class: table_list::TOOLBAR_ACTIONS,
                        button { class: button_class(ButtonVariant::Secondary), "Export" }
                        button { class: button_class(ButtonVariant::Secondary), "Refresh" }
                    }
                }
                div {
                    class: table_list::MOBILE_LIST,
                    MobileRecord {
                        eyebrow: "online",
                        title: "node-1",
                        detail: "host-a - linux - 30 sec ago"
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
                                th { "Last Seen" }
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
                                td { class: "text-terminal-muted", "30 sec ago" }
                                td {
                                    class: "flex gap-2",
                                    button { class: button_class(ButtonVariant::Secondary), "Details" }
                                    button { class: button_class(ButtonVariant::Danger), "Revoke" }
                                }
                            }
                        }
                    }
                }
                TablePagination { label: "1-1 of 1 nodes" }
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
                    class: table_list::MOBILE_LIST,
                    MobileRecord {
                        eyebrow: "enabled",
                        title: "Laptop passkey",
                        detail: "credential-1"
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
                div {
                    class: table_list::MOBILE_LIST,
                    MobileRecord {
                        eyebrow: "Admin",
                        title: "admin@example.com",
                        detail: "Operations"
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
                class: table_list::MOBILE_LIST,
                for row in rows.clone() {
                    MobileRecord {
                        eyebrow: row.first().copied().unwrap_or("Record"),
                        title: row.get(1).copied().unwrap_or("Pending"),
                        detail: row.get(2).copied().unwrap_or("Awaiting data")
                    }
                }
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

#[component]
fn MobileRecord(eyebrow: &'static str, title: &'static str, detail: &'static str) -> Element {
    rsx! {
        div {
            class: table_list::MOBILE_RECORD,
            p {
                class: "m-0 text-xs font-semibold text-lightning-cyan",
                "{eyebrow}"
            }
            p {
                class: "m-0 text-sm font-semibold text-terminal-text",
                "{title}"
            }
            p {
                class: "m-0 text-xs text-terminal-muted",
                "{detail}"
            }
        }
    }
}

#[component]
fn MetricCard(label: &'static str, value: &'static str, detail: &'static str) -> Element {
    rsx! {
        div {
            class: "sunbolt-metric-card",
            p {
                class: "m-0 text-xs font-semibold text-terminal-muted",
                "{label}"
            }
            strong {
                class: "sunbolt-metric-value",
                "{value}"
            }
            p {
                class: "m-0 text-xs text-terminal-muted",
                "{detail}"
            }
        }
    }
}

#[component]
fn DashboardEvent(event: &'static str, actor: &'static str, detail: &'static str) -> Element {
    rsx! {
        div {
            class: table_list::DENSE_ROW,
            div {
                class: "min-w-0",
                p {
                    class: "m-0 font-mono text-xs text-lightning-cyan",
                    "{event}"
                }
                p {
                    class: "m-0 text-xs text-terminal-muted",
                    "{detail}"
                }
            }
            span {
                class: "text-xs text-terminal-text",
                "{actor}"
            }
        }
    }
}

#[component]
fn TablePagination(label: &'static str) -> Element {
    rsx! {
        div {
            class: table_list::PAGINATION,
            span { "{label}" }
            div {
                class: table_list::TOOLBAR_ACTIONS,
                button {
                    class: button_class(ButtonVariant::Secondary),
                    disabled: true,
                    "Previous"
                }
                button {
                    class: button_class(ButtonVariant::Secondary),
                    disabled: true,
                    "Next"
                }
            }
        }
    }
}
