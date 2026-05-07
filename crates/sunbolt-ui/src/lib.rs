use dioxus::prelude::*;
use sunbolt_protocol::TerminalSize;

/// DOM id used by the browser terminal bridge.
pub const TERMINAL_MOUNT_ID: &str = "sunbolt-terminal";
pub const TERMINAL_NODE_INPUT_ID: &str = "sunbolt-terminal-node";

/// WebSocket endpoint used by the terminal UI.
pub const TERMINAL_WS_ENDPOINT: &str = "/terminal/ws";
pub const AUTH_LOGIN_ENDPOINT: &str = "/auth/login";
pub const AUTH_ME_ENDPOINT: &str = "/auth/me";
pub const AUTH_TERMINAL_ACCESS_ENDPOINT: &str = "/auth/terminal-access";
pub const STEP_UP_MFA_ENDPOINT: &str = "/auth/mfa/step-up";
pub const CONTROL_PLANE_URL_CONFIG_GLOBAL: &str = "SUNBOLT_CONTROL_PLANE_URL";
pub const TERMINAL_WS_CONFIG_GLOBAL: &str = "SUNBOLT_TERMINAL_WS_URL";
pub const XTERM_SCRIPT_URL: &str =
    "https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js";
pub const XTERM_STYLESHEET_URL: &str =
    "https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.min.css";

const DEFAULT_TERMINAL_SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
const STATUS_BASE_CLASS: &str = "sunbolt-status border-terminal-border text-terminal-muted";
const STATUS_CONNECTING_CLASS: &str =
    "sunbolt-status border-sun-amber/70 bg-sun-amber/10 text-sun-amber";
const STATUS_CONNECTED_CLASS: &str =
    "sunbolt-status border-lightning-cyan/70 bg-lightning-cyan/10 text-lightning-cyan";
const STATUS_ERROR_CLASS: &str =
    "sunbolt-status border-warm-orange/70 bg-warm-orange/10 text-warm-orange";
const STATUS_CLOSED_CLASS: &str =
    "sunbolt-status border-terminal-border bg-terminal-bg/80 text-terminal-muted";
const ACTION_BUTTON_CLASS: &str = "sunbolt-button sunbolt-button-secondary";
const PRIMARY_BUTTON_CLASS: &str = "sunbolt-button sunbolt-button-primary";
const DANGER_BUTTON_CLASS: &str = "sunbolt-button sunbolt-button-danger";
const NAV_BUTTON_CLASS: &str = "sunbolt-nav-button";
const NAV_BUTTON_ACTIVE_CLASS: &str = "sunbolt-nav-button sunbolt-nav-button-active";
const FALLBACK_OUTPUT_CLASS: &str = "sunbolt-fallback-output";
const FALLBACK_INPUT_CLASS: &str = "sunbolt-fallback-input";
const TEXT_INPUT_CLASS: &str = "sunbolt-input";
const CARD_CLASS: &str = "sunbolt-card";
const TABLE_WRAP_CLASS: &str = "sunbolt-table-wrap";
const TABLE_CLASS: &str = "sunbolt-table";

/// Returns the display title for the web UI shell.
#[must_use]
pub fn app_title() -> String {
    sunbolt_common::product_name().to_owned()
}

fn control_plane_config_script() -> Option<String> {
    browser_config_script(option_env!("SUNBOLT_CONTROL_PLANE_URL"))
}

fn browser_config_script(control_plane_url: Option<&str>) -> Option<String> {
    let control_plane_url = control_plane_url?.trim();
    if control_plane_url.is_empty() {
        return None;
    }

    Some(format!(
        r#"window.{CONTROL_PLANE_URL_CONFIG_GLOBAL} = window.{CONTROL_PLANE_URL_CONFIG_GLOBAL} || "{control_plane_url}";"#
    ))
}

/// Root Dioxus app for the Sunbolt web UI.
#[component]
pub fn App() -> Element {
    let mut page = use_signal(|| ShellPage::Terminal);

    rsx! {
        main {
            class: "sunbolt-shell",
            section {
                class: "sunbolt-app-grid",
                header {
                    class: "sunbolt-topbar",
                    div {
                        class: "sunbolt-brand",
                        div {
                            class: "sunbolt-brand-mark",
                            "S"
                        }
                        div {
                            class: "min-w-0",
                            h1 {
                                class: "m-0 text-xl font-black tracking-tight text-terminal-text",
                                "Sunbolt"
                            }
                            p {
                                class: "m-0 text-xs font-medium text-terminal-muted",
                                "Secure terminal control plane"
                            }
                        }
                    }
                    nav {
                        class: "sunbolt-nav",
                        "aria-label": "Primary navigation",
                        button {
                            class: if page() == ShellPage::Terminal {
                                NAV_BUTTON_ACTIVE_CLASS
                            } else {
                                NAV_BUTTON_CLASS
                            },
                            onclick: move |_| page.set(ShellPage::Terminal),
                            "Terminal"
                        }
                        button {
                            class: if page() == ShellPage::AccessHistory {
                                NAV_BUTTON_ACTIVE_CLASS
                            } else {
                                NAV_BUTTON_CLASS
                            },
                            onclick: move |_| page.set(ShellPage::AccessHistory),
                            "Access History"
                        }
                        button {
                            class: if page() == ShellPage::Nodes {
                                NAV_BUTTON_ACTIVE_CLASS
                            } else {
                                NAV_BUTTON_CLASS
                            },
                            onclick: move |_| page.set(ShellPage::Nodes),
                            "Nodes"
                        }
                        button {
                            class: if page() == ShellPage::AuditLogs {
                                NAV_BUTTON_ACTIVE_CLASS
                            } else {
                                NAV_BUTTON_CLASS
                            },
                            onclick: move |_| page.set(ShellPage::AuditLogs),
                            "Audit Logs"
                        }
                        button {
                            class: if page() == ShellPage::Security {
                                NAV_BUTTON_ACTIVE_CLASS
                            } else {
                                NAV_BUTTON_CLASS
                            },
                            onclick: move |_| page.set(ShellPage::Security),
                            "Security"
                        }
                        button {
                            class: if page() == ShellPage::Admin {
                                NAV_BUTTON_ACTIVE_CLASS
                            } else {
                                NAV_BUTTON_CLASS
                            },
                            onclick: move |_| page.set(ShellPage::Admin),
                            "Admin"
                        }
                    }
                }
                match page() {
                    ShellPage::Terminal => rsx! { TerminalPageBody {} },
                    ShellPage::AccessHistory => rsx! { AccessHistoryPage {} },
                    ShellPage::Nodes => rsx! { NodesPage {} },
                    ShellPage::AuditLogs => rsx! { AuditLogPage {} },
                    ShellPage::Security => rsx! { SecurityPage {} },
                    ShellPage::Admin => rsx! { AdminPage {} },
                }
            }
            link {
                rel: "stylesheet",
                href: asset!("/assets/sunbolt.css")
            }
            if let Some(browser_config) = control_plane_config_script() {
                script {
                    dangerous_inner_html: browser_config
                }
            }
            script {
                src: XTERM_SCRIPT_URL
            }
            link {
                rel: "stylesheet",
                href: XTERM_STYLESHEET_URL
            }
            if page() == ShellPage::Terminal {
                script {
                    dangerous_inner_html: terminal_bridge_script()
                }
            }
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ShellPage {
    Terminal,
    AccessHistory,
    Nodes,
    AuditLogs,
    Security,
    Admin,
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
                    input {
                        id: TERMINAL_NODE_INPUT_ID,
                        class: TEXT_INPUT_CLASS,
                        placeholder: "node id, empty for local",
                        value: ""
                    }
                    div {
                        id: "sunbolt-terminal-status",
                        class: STATUS_CONNECTING_CLASS,
                        "Connecting"
                    }
                    button {
                        id: "sunbolt-terminal-mfa",
                        class: PRIMARY_BUTTON_CLASS,
                        "Step-up MFA"
                    }
                    button {
                        id: "sunbolt-terminal-reconnect",
                        class: ACTION_BUTTON_CLASS,
                        disabled: true,
                        "Reconnect"
                    }
                    button {
                        id: "sunbolt-terminal-retry",
                        class: ACTION_BUTTON_CLASS,
                        "Retry"
                    }
                    button {
                        id: "sunbolt-terminal-close",
                        class: DANGER_BUTTON_CLASS,
                        "Close"
                    }
                }
            }
            div {
                id: "sunbolt-terminal-error",
                class: "sunbolt-alert hidden items-center",
                role: "status"
            }
            div {
                id: "sunbolt-terminal-auth",
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
                    class: TEXT_INPUT_CLASS,
                    placeholder: "email",
                    autocomplete: "username"
                }
                input {
                    id: "sunbolt-terminal-password",
                    class: TEXT_INPUT_CLASS,
                    placeholder: "password",
                    r#type: "password",
                    autocomplete: "current-password"
                }
                button {
                    id: "sunbolt-terminal-login",
                    class: PRIMARY_BUTTON_CLASS,
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
            class: "sunbolt-page",
            div {
                class: CARD_CLASS,
                h2 {
                    class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "Access History"
                }
                div {
                    class: TABLE_WRAP_CLASS,
                    table {
                        class: TABLE_CLASS,
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
            class: "sunbolt-page",
            div {
                class: CARD_CLASS,
                h2 {
                    class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                    "Audit Logs"
                }
                div {
                    class: TABLE_WRAP_CLASS,
                    table {
                        class: TABLE_CLASS,
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
                class: CARD_CLASS,
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
                class: TABLE_WRAP_CLASS,
                table {
                    class: TABLE_CLASS,
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
                                span { class: STATUS_CONNECTED_CLASS, "online" }
                            }
                            td {
                                class: "flex gap-2",
                                button { class: ACTION_BUTTON_CLASS, "Details" }
                                button { class: DANGER_BUTTON_CLASS, "Revoke" }
                            }
                        }
                    }
                }
            }
            div {
                class: CARD_CLASS,
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
                class: CARD_CLASS,
                div {
                    class: "mb-3 flex items-center justify-between gap-3",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Passkeys"
                    }
                    button {
                        class: PRIMARY_BUTTON_CLASS,
                        "Add passkey"
                    }
                }
                div {
                    class: TABLE_WRAP_CLASS,
                    table {
                        class: TABLE_CLASS,
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
                                    span { class: STATUS_CONNECTED_CLASS, "enabled" }
                                }
                                td {
                                    button { class: DANGER_BUTTON_CLASS, "Disable" }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: CARD_CLASS,
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
                class: CARD_CLASS,
                div {
                    class: "mb-3 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
                    h2 {
                        class: "m-0 text-lg font-black tracking-tight text-terminal-text",
                        "Workspace Access"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        button { class: PRIMARY_BUTTON_CLASS, "Add member" }
                        button { class: ACTION_BUTTON_CLASS, "Grant role" }
                    }
                }
                div {
                    class: TABLE_WRAP_CLASS,
                    table {
                        class: TABLE_CLASS,
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
                                    button { class: DANGER_BUTTON_CLASS, "Remove" }
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
            class: CARD_CLASS,
            h2 {
                class: "mb-3 mt-0 text-lg font-black tracking-tight text-terminal-text",
                "{title}"
            }
            div {
                class: TABLE_WRAP_CLASS,
                table {
                    class: TABLE_CLASS,
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

/// Builds the browser bridge script with stable endpoint and terminal defaults.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn terminal_bridge_script() -> String {
    format!(
        r##"
(() => {{
  const mount = document.getElementById("{mount_id}");
  const status = document.getElementById("sunbolt-terminal-status");
  const errorDisplay = document.getElementById("sunbolt-terminal-error");
  const authPanel = document.getElementById("sunbolt-terminal-auth");
  const emailInput = document.getElementById("sunbolt-terminal-email");
  const passwordInput = document.getElementById("sunbolt-terminal-password");
  const loginButton = document.getElementById("sunbolt-terminal-login");
  const closeButton = document.getElementById("sunbolt-terminal-close");
  const mfaButton = document.getElementById("sunbolt-terminal-mfa");
  const reconnectButton = document.getElementById("sunbolt-terminal-reconnect");
  const retryButton = document.getElementById("sunbolt-terminal-retry");
  const nodeInput = document.getElementById("{node_input_id}");
  if (!mount || mount.dataset.sunboltTerminalReady === "true") {{
    return;
  }}
  mount.dataset.sunboltTerminalReady = "true";

  let sessionId = null;
  let cols = {cols};
  let rows = {rows};
  let reconnectToken = null;
  let socket = null;
  let terminal = null;
  let fallbackInput = null;
  let fallbackOutput = null;
  let resizeObserver = null;
  let mountObserver = null;
  let cleanedUp = false;
  let authenticated = false;
  let loginBusy = false;
  let currentStatusState = "idle";

  const terminalData = (data) => {{
    if (typeof data !== "string") {{
      return "";
    }}
    return data.replace(/\u0000/g, "");
  }};

  const normalizeBaseUrl = (value) => String(value || "").trim().replace(/\/+$/, "");

  const controlPlaneBaseUrl = () => {{
    if (window.{control_plane_config_global}) {{
      return normalizeBaseUrl(window.{control_plane_config_global});
    }}
    if ((window.location.hostname === "127.0.0.1" || window.location.hostname === "localhost")
      && window.location.port !== "3000") {{
      return `${{window.location.protocol}}//${{window.location.hostname}}:3000`;
    }}
    return normalizeBaseUrl(window.location.origin);
  }};

  const httpEndpointUrl = (path) => {{
    if (path.startsWith("http://") || path.startsWith("https://")) {{
      return path;
    }}
    return `${{controlPlaneBaseUrl()}}${{path}}`;
  }};

  const setError = (message) => {{
    if (!errorDisplay) {{
      return;
    }}
    errorDisplay.textContent = message || "";
    errorDisplay.classList.toggle("hidden", !message);
    errorDisplay.classList.toggle("flex", Boolean(message));
  }};

  const syncControls = () => {{
    if (mfaButton) {{
      mfaButton.disabled = !authenticated || loginBusy || currentStatusState === "connecting";
    }}
    if (closeButton) {{
      closeButton.disabled = !authenticated
        || currentStatusState === "idle"
        || currentStatusState === "closed"
        || currentStatusState === "disconnected";
    }}
    if (reconnectButton) {{
      reconnectButton.disabled = !(
        currentStatusState === "disconnected" && sessionId && reconnectToken
      );
    }}
    if (retryButton) {{
      retryButton.disabled = loginBusy
        || currentStatusState === "connecting"
        || currentStatusState === "connected";
    }}
    if (nodeInput) {{
      nodeInput.disabled = !authenticated || loginBusy || currentStatusState === "connecting";
    }}
  }};

  const setStatus = (label, state) => {{
    currentStatusState = state;
    if (!status) {{
      syncControls();
      return;
    }}
    status.textContent = label;
    const classes = {{
      idle: "{status_closed_class}",
      connecting: "{status_connecting_class}",
      connected: "{status_connected_class}",
      error: "{status_error_class}",
      disconnected: "{status_closed_class}",
      closed: "{status_closed_class}"
    }};
    status.className = classes[state] || "{status_base_class}";
    syncControls();
  }};

  const setAuthVisible = (visible) => {{
    if (!authPanel) {{
      return;
    }}
    authPanel.classList.toggle("hidden", !visible);
    authPanel.classList.toggle("flex", visible);
    if (visible && emailInput && !emailInput.value) {{
      emailInput.focus();
    }}
  }};

  const setLoginBusy = (busy) => {{
    loginBusy = busy;
    if (loginButton) {{
      loginButton.disabled = busy;
    }}
    if (emailInput) {{
      emailInput.disabled = busy;
    }}
    if (passwordInput) {{
      passwordInput.disabled = busy;
    }}
    syncControls();
  }};

  const writeOutput = (data) => {{
    const safeData = terminalData(data);
    if (terminal) {{
      terminal.write(safeData);
      return;
    }}
    if (fallbackOutput) {{
      fallbackOutput.textContent += safeData;
      fallbackOutput.scrollTop = fallbackOutput.scrollHeight;
    }}
  }};

  const send = (message) => {{
    if (socket && socket.readyState === WebSocket.OPEN) {{
      socket.send(JSON.stringify(message));
    }}
  }};

  const terminalWebSocketUrl = () => {{
    if (window.{ws_config_global}) {{
      return window.{ws_config_global};
    }}
    const configured = mount.dataset.wsEndpoint || "{endpoint}";
    if (configured.startsWith("ws://") || configured.startsWith("wss://")) {{
      return configured;
    }}
    if (configured.startsWith("http://")) {{
      return `ws://${{configured.slice("http://".length)}}`;
    }}
    if (configured.startsWith("https://")) {{
      return `wss://${{configured.slice("https://".length)}}`;
    }}
    const controlPlaneUrl = controlPlaneBaseUrl();
    if (controlPlaneUrl.startsWith("http://")) {{
      return `ws://${{controlPlaneUrl.slice("http://".length)}}${{configured}}`;
    }}
    if (controlPlaneUrl.startsWith("https://")) {{
      return `wss://${{controlPlaneUrl.slice("https://".length)}}${{configured}}`;
    }}
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${{protocol}}//${{window.location.host}}${{configured}}`;
  }};

  const resize = () => {{
    const rect = mount.getBoundingClientRect();
    cols = Math.max(20, Math.floor(rect.width / 9));
    rows = Math.max(6, Math.floor(rect.height / 18));
    if (terminal && typeof terminal.resize === "function") {{
      terminal.resize(cols, rows);
    }}
    if (sessionId) {{
      send({{ type: "resize", session_id: sessionId, size: {{ cols, rows }} }});
    }}
  }};

  const readErrorMessage = async (response, fallbackMessage) => {{
    try {{
      const payload = await response.json();
      if (payload && typeof payload.error === "string" && payload.error) {{
        return payload.error;
      }}
    }} catch (_error) {{}}
    return fallbackMessage;
  }};

  const ensureTerminalAccess = async () => {{
    const response = await fetch(httpEndpointUrl("{auth_terminal_access_endpoint}"), {{
      credentials: "include"
    }});
    if (response.ok) {{
      return true;
    }}
    authenticated = response.status !== 401;
    if (response.status === 401) {{
      setAuthVisible(true);
      setStatus("Login Required", "idle");
    }} else if (response.status === 403) {{
      setStatus("MFA Required", "error");
    }} else {{
      setStatus("Error", "error");
    }}
    setError(await readErrorMessage(response, "Unable to verify terminal access."));
    return false;
  }};

  const ensureAuthenticatedSession = async () => {{
    if (authenticated) {{
      return true;
    }}

    setStatus("Checking Session", "connecting");
    setError("");
    try {{
      const response = await fetch(httpEndpointUrl("{auth_me_endpoint}"), {{
        credentials: "include"
      }});
      if (response.ok) {{
        authenticated = true;
        setAuthVisible(false);
        setStatus("Idle", "idle");
        return true;
      }}
      authenticated = false;
      if (response.status === 401) {{
        setAuthVisible(true);
        setStatus("Login Required", "idle");
        setError("Sign in before opening a terminal.");
        return false;
      }}
      setAuthVisible(true);
      setStatus("Error", "error");
      setError(await readErrorMessage(response, "Unable to verify the current session."));
      return false;
    }} catch (_error) {{
      authenticated = false;
      setAuthVisible(true);
      setStatus("Error", "error");
      setError("Unable to reach the control plane.");
      return false;
    }}
  }};

  const connect = async (reattach = false) => {{
    if (!(await ensureAuthenticatedSession())) {{
      return;
    }}
    if (!(await ensureTerminalAccess())) {{
      return;
    }}

    setStatus("Connecting", "connecting");
    setError("");
    const url = terminalWebSocketUrl();
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {{
      socket.close();
    }}
    socket = new WebSocket(url);

    socket.addEventListener("open", () => {{
      setStatus("Connected", "connected");
      resize();
      if (reattach && sessionId && reconnectToken) {{
        send({{
          type: "reattach",
          session_id: sessionId,
          reconnect_token: reconnectToken
        }});
      }} else {{
        send({{
          type: "start",
          node_id: nodeInput && nodeInput.value.trim() ? nodeInput.value.trim() : null,
          initial_size: {{ cols, rows }}
        }});
      }}
    }});

    socket.addEventListener("message", (event) => {{
      let message;
      try {{
        message = JSON.parse(event.data);
      }} catch (_error) {{
        setStatus("Error", "error");
        setError("Terminal server sent an invalid message.");
        return;
      }}
      if (message.type === "started") {{
        sessionId = message.session_id;
        reconnectToken = message.reconnect_token || null;
        setStatus("Active", "connected");
      }} else if (message.type === "reattached") {{
        sessionId = message.session_id;
        reconnectToken = message.reconnect_token || reconnectToken;
        setStatus("Active", "connected");
      }} else if (message.type === "detached") {{
        setStatus("Closed", "closed");
      }} else if (message.type === "output") {{
        writeOutput(message.data);
      }} else if (message.type === "error") {{
        setStatus("Error", "error");
        const errorMessage = message.error && message.error.message
          ? message.error.message
          : "Terminal connection failed.";
        setError(errorMessage);
        writeOutput(`\r\n${{errorMessage}}\r\n`);
      }} else if (message.type === "exited") {{
        setStatus("Closed", "closed");
      }}
    }});

    socket.addEventListener("close", () => {{
      setStatus("Disconnected", "disconnected");
    }});

    socket.addEventListener("error", () => {{
      setStatus("Error", "error");
      setError("Unable to connect to the terminal WebSocket.");
    }});
  }};

  const login = async () => {{
    if (!emailInput || !passwordInput) {{
      return;
    }}
    const email = emailInput.value.trim();
    const password = passwordInput.value;
    if (!email || !password) {{
      setStatus("Login Required", "error");
      setError("Email and password are required.");
      return;
    }}

    setLoginBusy(true);
    setStatus("Signing In", "connecting");
    setError("");
    try {{
      const response = await fetch(httpEndpointUrl("{auth_login_endpoint}"), {{
        method: "POST",
        credentials: "include",
        headers: {{ "content-type": "application/json" }},
        body: JSON.stringify({{ email, password }})
      }});
      if (!response.ok) {{
        authenticated = false;
        setAuthVisible(true);
        setStatus("Login Required", "error");
        setError(await readErrorMessage(response, "Login failed."));
        return;
      }}
      passwordInput.value = "";
      authenticated = false;
      if (await ensureAuthenticatedSession()) {{
        connect();
      }}
    }} catch (_error) {{
      authenticated = false;
      setAuthVisible(true);
      setStatus("Error", "error");
      setError("Unable to sign in to the control plane.");
    }} finally {{
      setLoginBusy(false);
    }}
  }};

  const sendInput = (data) => {{
    if (!sessionId) {{
      return;
    }}
    send({{ type: "input", session_id: sessionId, data: terminalData(data) }});
  }};

  const closeTerminal = () => {{
    if (sessionId) {{
      send({{ type: "close", session_id: sessionId }});
    }}
    reconnectToken = null;
    if (socket) {{
      socket.close();
    }}
    setStatus("Closed", "closed");
  }};

  const cleanupTerminal = () => {{
    if (cleanedUp) {{
      return;
    }}
    cleanedUp = true;
    if (sessionId && reconnectToken) {{
      send({{ type: "detach", session_id: sessionId }});
    }} else if (sessionId) {{
      send({{ type: "close", session_id: sessionId }});
    }}
    if (socket) {{
      socket.close();
    }}
    if (resizeObserver) {{
      resizeObserver.disconnect();
    }}
    if (mountObserver) {{
      mountObserver.disconnect();
    }}
  }};

  const completeStepUpMfa = async () => {{
    if (!(await ensureAuthenticatedSession())) {{
      return;
    }}
    setStatus("MFA", "connecting");
    setError("");
    try {{
      const response = await fetch(httpEndpointUrl("{step_up_mfa_endpoint}"), {{
        method: "POST",
        credentials: "include",
        headers: {{ "content-type": "application/json" }},
        body: JSON.stringify({{ factor_type: "totp" }})
      }});
      if (!response.ok) {{
        if (response.status === 401) {{
          authenticated = false;
          setAuthVisible(true);
        }}
        setStatus("MFA Required", "error");
        setError(await readErrorMessage(response, "Step-up MFA request failed."));
        return;
      }}
      connect();
    }} catch (_error) {{
      setStatus("Error", "error");
      setError("Unable to complete step-up MFA.");
    }}
  }};

  if (window.Terminal) {{
    mount.dataset.sunboltTerminalRenderer = "xterm";
    terminal = new window.Terminal({{
      cursorBlink: true,
      convertEol: true,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
      fontSize: 14,
      theme: {{
        background: "#09090B",
        foreground: "#FAFAFA",
        cursor: "#22D3EE"
      }}
    }});
    terminal.open(mount);
    terminal.focus();
    mount.addEventListener("click", () => terminal.focus());
    terminal.onData(sendInput);
  }} else {{
    mount.dataset.sunboltTerminalRenderer = "textarea";
    mount.innerHTML = "";
    fallbackOutput = document.createElement("pre");
    fallbackOutput.className = "{fallback_output_class}";
    fallbackInput = document.createElement("textarea");
    fallbackInput.className = "{fallback_input_class}";
    fallbackInput.spellcheck = false;
    fallbackInput.addEventListener("input", () => {{
      sendInput(fallbackInput.value);
      fallbackInput.value = "";
    }});
    mount.append(fallbackOutput, fallbackInput);
    fallbackInput.focus();
  }}

  resizeObserver = new ResizeObserver(resize);
  resizeObserver.observe(mount);
  mountObserver = new MutationObserver(() => {{
    if (!document.body.contains(mount)) {{
      cleanupTerminal();
    }}
  }});
  mountObserver.observe(document.body, {{ childList: true, subtree: true }});
  if (closeButton) {{
    closeButton.addEventListener("click", closeTerminal);
  }}
  if (mfaButton) {{
    mfaButton.addEventListener("click", completeStepUpMfa);
  }}
  if (loginButton) {{
    loginButton.addEventListener("click", login);
  }}
  for (const field of [emailInput, passwordInput]) {{
    if (!field) {{
      continue;
    }}
    field.addEventListener("keydown", (event) => {{
      if (event.key === "Enter") {{
        event.preventDefault();
        login();
      }}
    }});
  }}
  if (reconnectButton) {{
    reconnectButton.addEventListener("click", () => {{
      if (!sessionId || !reconnectToken) {{
        return;
      }}
      setStatus("Reconnecting", "connecting");
      connect(true);
    }});
  }}
  if (retryButton) {{
    retryButton.addEventListener("click", () => {{
      connect();
    }});
  }}
  window.addEventListener("beforeunload", cleanupTerminal);
  window.addEventListener("pagehide", cleanupTerminal);
  setStatus("Idle", "idle");
  setAuthVisible(false);
  connect();
}})();
"##,
        mount_id = TERMINAL_MOUNT_ID,
        node_input_id = TERMINAL_NODE_INPUT_ID,
        endpoint = TERMINAL_WS_ENDPOINT,
        auth_login_endpoint = AUTH_LOGIN_ENDPOINT,
        auth_me_endpoint = AUTH_ME_ENDPOINT,
        auth_terminal_access_endpoint = AUTH_TERMINAL_ACCESS_ENDPOINT,
        step_up_mfa_endpoint = STEP_UP_MFA_ENDPOINT,
        control_plane_config_global = CONTROL_PLANE_URL_CONFIG_GLOBAL,
        ws_config_global = TERMINAL_WS_CONFIG_GLOBAL,
        cols = DEFAULT_TERMINAL_SIZE.cols,
        rows = DEFAULT_TERMINAL_SIZE.rows,
        status_base_class = STATUS_BASE_CLASS,
        status_connecting_class = STATUS_CONNECTING_CLASS,
        status_connected_class = STATUS_CONNECTED_CLASS,
        status_error_class = STATUS_ERROR_CLASS,
        status_closed_class = STATUS_CLOSED_CLASS,
        fallback_output_class = FALLBACK_OUTPUT_CLASS,
        fallback_input_class = FALLBACK_INPUT_CLASS,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        app_title, browser_config_script, terminal_bridge_script, AUTH_LOGIN_ENDPOINT,
        AUTH_ME_ENDPOINT, AUTH_TERMINAL_ACCESS_ENDPOINT, CONTROL_PLANE_URL_CONFIG_GLOBAL,
        STEP_UP_MFA_ENDPOINT, TERMINAL_MOUNT_ID, TERMINAL_NODE_INPUT_ID, TERMINAL_WS_CONFIG_GLOBAL,
        TERMINAL_WS_ENDPOINT, XTERM_SCRIPT_URL, XTERM_STYLESHEET_URL,
    };

    #[test]
    fn app_title_uses_product_name() {
        assert_eq!(app_title(), "Sunbolt");
    }

    #[test]
    fn browser_config_script_sets_control_plane_global() {
        let script = browser_config_script(Some("http://127.0.0.1:3000"))
            .expect("script should be generated");

        assert!(script.contains(CONTROL_PLANE_URL_CONFIG_GLOBAL));
        assert!(script.contains("http://127.0.0.1:3000"));
    }

    #[test]
    fn browser_config_script_ignores_empty_urls() {
        assert!(browser_config_script(Some("   ")).is_none());
        assert!(browser_config_script(None).is_none());
    }

    #[test]
    fn terminal_bridge_uses_terminal_websocket_endpoint() {
        let script = terminal_bridge_script();

        assert!(script.contains(TERMINAL_WS_ENDPOINT));
        assert!(script.contains(AUTH_LOGIN_ENDPOINT));
        assert!(script.contains(AUTH_ME_ENDPOINT));
        assert!(script.contains(AUTH_TERMINAL_ACCESS_ENDPOINT));
        assert!(script.contains(STEP_UP_MFA_ENDPOINT));
        assert!(script.contains(TERMINAL_MOUNT_ID));
        assert!(script.contains(TERMINAL_NODE_INPUT_ID));
        assert!(script.contains(r#"type: "start""#));
        assert!(script.contains(r#"type: "input""#));
        assert!(script.contains(r#"type: "resize""#));
        assert!(script.contains(r#"type: "close""#));
        assert!(script.contains("sunbolt-terminal-close"));
        assert!(script.contains("sunbolt-terminal-mfa"));
        assert!(script.contains("sunbolt-terminal-reconnect"));
        assert!(script.contains("border-lightning-cyan"));
    }

    #[test]
    fn terminal_bridge_uses_xterm_renderer() {
        let script = terminal_bridge_script();

        assert!(XTERM_SCRIPT_URL.contains("@xterm/xterm@5.5.0"));
        assert!(XTERM_STYLESHEET_URL.contains("@xterm/xterm@5.5.0"));
        assert!(script.contains("window.Terminal"));
        assert!(script.contains("terminal.open(mount)"));
        assert!(script.contains("terminal.write(safeData)"));
        assert!(script.contains("terminal.focus()"));
        assert!(script.contains("sunboltTerminalRenderer"));
    }

    #[test]
    fn terminal_bridge_exposes_websocket_client_states() {
        let script = terminal_bridge_script();

        assert!(script.contains(CONTROL_PLANE_URL_CONFIG_GLOBAL));
        assert!(script.contains(TERMINAL_WS_CONFIG_GLOBAL));
        assert!(script.contains("controlPlaneBaseUrl"));
        assert!(script.contains(r#"window.location.port !== "3000""#));
        assert!(script.contains("httpEndpointUrl"));
        assert!(script.contains("ensureTerminalAccess"));
        assert!(script.contains("terminalWebSocketUrl"));
        assert!(script.contains("new WebSocket(url)"));
        assert!(script.contains(r#"setStatus("Idle", "idle")"#));
        assert!(script.contains(r#"setStatus("Checking Session", "connecting")"#));
        assert!(script.contains(r#"setStatus("Login Required", "idle")"#));
        assert!(script.contains(r#"setStatus("MFA Required", "error")"#));
        assert!(script.contains(r#"setStatus("Connecting", "connecting")"#));
        assert!(script.contains(r#"setStatus("Active", "connected")"#));
        assert!(script.contains(r#"setStatus("Disconnected", "disconnected")"#));
        assert!(script.contains(r#"setStatus("Error", "error")"#));
        assert!(script.contains("sunbolt-terminal-error"));
        assert!(script.contains("sunbolt-terminal-retry"));
    }

    #[test]
    fn terminal_bridge_requires_auth_before_opening_websocket() {
        let script = terminal_bridge_script();

        assert!(script.contains("ensureAuthenticatedSession"));
        assert!(script.contains(r#"fetch(httpEndpointUrl("/auth/me")"#));
        assert!(script.contains(r#"fetch(httpEndpointUrl("/auth/login")"#));
        assert!(script.contains("credentials: \"include\""));
        assert!(script.contains("Sign in before opening a terminal."));
    }

    #[test]
    fn terminal_bridge_sanitizes_terminal_data_without_blocking_control_input() {
        let script = terminal_bridge_script();

        assert!(script.contains("terminalData"));
        assert!(script.contains(r#"replace(/\u0000/g, "")"#));
        assert!(script.contains("terminal.onData(sendInput)"));
        assert!(script.contains(r#"type: "input""#));
        assert!(script.contains("writeOutput(message.data)"));
    }

    #[test]
    fn terminal_bridge_handles_resize_and_cleanup() {
        let script = terminal_bridge_script();

        assert!(script.contains("new ResizeObserver(resize)"));
        assert!(script.contains(r#"type: "resize""#));
        assert!(script.contains("new MutationObserver"));
        assert!(script.contains("cleanupTerminal"));
        assert!(script.contains(r#"type: "detach""#));
        assert!(script.contains(r#"type: "close""#));
        assert!(script.contains("pagehide"));
        assert!(script.contains("beforeunload"));
    }
}
