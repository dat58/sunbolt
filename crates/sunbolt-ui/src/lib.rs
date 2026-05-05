use dioxus::prelude::*;
use sunbolt_protocol::TerminalSize;

/// DOM id used by the browser terminal bridge.
pub const TERMINAL_MOUNT_ID: &str = "sunbolt-terminal";
pub const TERMINAL_NODE_INPUT_ID: &str = "sunbolt-terminal-node";

/// WebSocket endpoint used by the terminal UI.
pub const TERMINAL_WS_ENDPOINT: &str = "/terminal/ws";
pub const STEP_UP_MFA_ENDPOINT: &str = "/auth/mfa/step-up";

const DEFAULT_TERMINAL_SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
const STATUS_BASE_CLASS: &str = "inline-flex h-6 items-center rounded-full border px-2.5 text-xs";
const STATUS_CONNECTING_CLASS: &str =
    "inline-flex h-6 items-center rounded-full border px-2.5 text-xs border-terminal-border text-terminal-muted";
const STATUS_CONNECTED_CLASS: &str =
    "inline-flex h-6 items-center rounded-full border px-2.5 text-xs border-lightning-cyan text-lightning-cyan";
const STATUS_ERROR_CLASS: &str =
    "inline-flex h-6 items-center rounded-full border px-2.5 text-xs border-warm-orange text-warm-orange";
const STATUS_CLOSED_CLASS: &str =
    "inline-flex h-6 items-center rounded-full border px-2.5 text-xs border-terminal-border text-terminal-muted";
const ACTION_BUTTON_CLASS: &str =
    "inline-flex h-7 items-center border border-terminal-border bg-terminal-bg px-3 text-xs text-terminal-text hover:border-lightning-cyan hover:text-lightning-cyan disabled:cursor-not-allowed disabled:opacity-40";
const NAV_BUTTON_CLASS: &str =
    "inline-flex h-7 items-center border border-terminal-border bg-terminal-bg px-3 text-xs text-terminal-muted hover:border-lightning-cyan hover:text-terminal-text";
const NAV_BUTTON_ACTIVE_CLASS: &str =
    "inline-flex h-7 items-center border border-lightning-cyan bg-terminal-bg px-3 text-xs text-lightning-cyan";
const FALLBACK_OUTPUT_CLASS: &str =
    "box-border h-[calc(100%-72px)] m-0 overflow-auto whitespace-pre-wrap font-mono text-sm";
const FALLBACK_INPUT_CLASS: &str =
    "mt-3 h-14 w-full resize-none box-border border border-terminal-border bg-terminal-surface font-mono text-sm text-terminal-text";
const TEXT_INPUT_CLASS: &str =
    "h-8 w-52 border border-terminal-border bg-terminal-bg px-2 text-xs text-terminal-text outline-none focus:border-lightning-cyan";

/// Returns the display title for the web UI shell.
#[must_use]
pub fn app_title() -> String {
    sunbolt_common::product_name().to_owned()
}

/// Root Dioxus app for the Sunbolt web UI.
#[component]
pub fn App() -> Element {
    let mut page = use_signal(|| ShellPage::Terminal);

    rsx! {
        main {
            class: "min-h-screen bg-terminal-bg font-sans text-terminal-text",
            section {
                class: "grid min-h-screen grid-rows-[48px_minmax(0,1fr)]",
                header {
                    class: "flex items-center justify-between border-b border-terminal-border bg-terminal-surface px-4",
                    h1 {
                        class: "m-0 text-[15px] font-bold text-sun-amber",
                        "Sunbolt"
                    }
                    nav {
                        class: "flex items-center gap-2",
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
                    }
                }
                match page() {
                    ShellPage::Terminal => rsx! { TerminalPageBody {} },
                    ShellPage::AccessHistory => rsx! { AccessHistoryPage {} },
                    ShellPage::Nodes => rsx! { NodesPage {} },
                    ShellPage::AuditLogs => rsx! { AuditLogPage {} },
                    ShellPage::Security => rsx! { SecurityPage {} },
                }
            }
            link {
                rel: "stylesheet",
                href: "/assets/sunbolt.css"
            }
            script {
                src: "https://cdn.jsdelivr.net/npm/xterm@5.5.0/lib/xterm.min.js"
            }
            link {
                rel: "stylesheet",
                href: "https://cdn.jsdelivr.net/npm/xterm@5.5.0/css/xterm.min.css"
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
}

/// First local terminal page.
#[component]
pub fn TerminalPageBody() -> Element {
    rsx! {
        section {
            class: "grid min-h-0 grid-rows-[48px_minmax(0,1fr)]",
            div {
                class: "flex items-center justify-end border-b border-terminal-border bg-terminal-surface px-4",
                div {
                    class: "flex items-center gap-2",
                    input {
                        id: TERMINAL_NODE_INPUT_ID,
                        class: TEXT_INPUT_CLASS,
                        placeholder: "node id",
                        value: ""
                    }
                    div {
                        id: "sunbolt-terminal-status",
                        class: STATUS_CONNECTING_CLASS,
                        "Connecting"
                    }
                    button {
                        id: "sunbolt-terminal-mfa",
                        class: ACTION_BUTTON_CLASS,
                        "Step-up MFA"
                    }
                    button {
                        id: "sunbolt-terminal-reconnect",
                        class: ACTION_BUTTON_CLASS,
                        disabled: true,
                        "Reconnect"
                    }
                    button {
                        id: "sunbolt-terminal-close",
                        class: ACTION_BUTTON_CLASS,
                        "Close"
                    }
                }
            }
            div {
                id: TERMINAL_MOUNT_ID,
                class: "min-h-0 overflow-hidden p-3 [&_.xterm]:h-full",
                tabindex: "0",
                "Terminal loading"
            }
        }
    }
}

#[component]
pub fn AccessHistoryPage() -> Element {
    rsx! {
        section {
            class: "p-4",
            h2 {
                class: "mb-3 mt-0 text-sm font-semibold text-terminal-text",
                "Access History"
            }
            div {
                class: "overflow-auto border border-terminal-border",
                table {
                    class: "w-full border-collapse text-sm",
                    thead {
                        class: "bg-terminal-surface text-terminal-muted",
                        tr {
                            th { class: "p-2 text-left font-medium", "Time" }
                            th { class: "p-2 text-left font-medium", "Event" }
                            th { class: "p-2 text-left font-medium", "Actor" }
                            th { class: "p-2 text-left font-medium", "Message" }
                        }
                    }
                    tbody {
                        tr {
                            td { class: "p-2 text-terminal-muted", "Pending" }
                            td { class: "p-2 text-terminal-text", "user.login.success" }
                            td { class: "p-2 text-terminal-text", "admin@example.com" }
                            td { class: "p-2 text-terminal-muted", "Awaiting backend list wiring" }
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
            class: "p-4",
            h2 {
                class: "mb-3 mt-0 text-sm font-semibold text-terminal-text",
                "Audit Logs"
            }
            div {
                class: "overflow-auto border border-terminal-border",
                table {
                    class: "w-full border-collapse text-sm",
                    thead {
                        class: "bg-terminal-surface text-terminal-muted",
                        tr {
                            th { class: "p-2 text-left font-medium", "Time" }
                            th { class: "p-2 text-left font-medium", "Kind" }
                            th { class: "p-2 text-left font-medium", "Actor" }
                            th { class: "p-2 text-left font-medium", "Message" }
                        }
                    }
                    tbody {
                        tr {
                            td { class: "p-2 text-terminal-muted", "Pending" }
                            td { class: "p-2 text-terminal-text", "terminal.opened" }
                            td { class: "p-2 text-terminal-text", "admin@example.com" }
                            td { class: "p-2 text-terminal-muted", "Awaiting backend list wiring" }
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
            class: "grid gap-4 p-4",
            div {
                class: "border border-terminal-border bg-terminal-surface p-4",
                h2 {
                    class: "mb-3 mt-0 text-sm font-semibold text-terminal-text",
                    "Node Enrollment"
                }
                pre {
                    class: "m-0 overflow-auto border border-terminal-border bg-terminal-bg p-3 font-mono text-xs text-lightning-cyan",
                    "SUNBOLT_CONTROL_PLANE_URL=http://127.0.0.1:3000 SUNBOLT_AGENT_ENROLLMENT_TOKEN=<token> cargo run -p sunbolt-agent"
                }
            }
            div {
                class: "overflow-auto border border-terminal-border",
                table {
                    class: "w-full border-collapse text-sm",
                    thead {
                        class: "bg-terminal-surface text-terminal-muted",
                        tr {
                            th { class: "p-2 text-left font-medium", "Node" }
                            th { class: "p-2 text-left font-medium", "Hostname" }
                            th { class: "p-2 text-left font-medium", "OS" }
                            th { class: "p-2 text-left font-medium", "Status" }
                            th { class: "p-2 text-left font-medium", "Actions" }
                        }
                    }
                    tbody {
                        tr {
                            td { class: "p-2 text-terminal-text", "node-1" }
                            td { class: "p-2 text-terminal-text", "host-a" }
                            td { class: "p-2 text-terminal-text", "linux" }
                            td { class: "p-2 text-lightning-cyan", "online" }
                            td {
                                class: "flex gap-2 p-2",
                                button { class: ACTION_BUTTON_CLASS, "Details" }
                                button { class: ACTION_BUTTON_CLASS, "Revoke" }
                            }
                        }
                    }
                }
            }
            div {
                class: "border border-terminal-border bg-terminal-surface p-4",
                h3 {
                    class: "mb-3 mt-0 text-sm font-semibold text-terminal-text",
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
            class: "grid gap-4 p-4",
            div {
                class: "border border-terminal-border bg-terminal-surface p-4",
                div {
                    class: "mb-3 flex items-center justify-between gap-3",
                    h2 {
                        class: "m-0 text-sm font-semibold text-terminal-text",
                        "Passkeys"
                    }
                    button {
                        class: ACTION_BUTTON_CLASS,
                        "Add passkey"
                    }
                }
                div {
                    class: "overflow-auto border border-terminal-border",
                    table {
                        class: "w-full border-collapse text-sm",
                        thead {
                            class: "bg-terminal-bg text-terminal-muted",
                            tr {
                                th { class: "p-2 text-left font-medium", "Label" }
                                th { class: "p-2 text-left font-medium", "Credential" }
                                th { class: "p-2 text-left font-medium", "Status" }
                                th { class: "p-2 text-left font-medium", "Actions" }
                            }
                        }
                        tbody {
                            tr {
                                td { class: "p-2 text-terminal-text", "Laptop passkey" }
                                td { class: "p-2 font-mono text-xs text-terminal-muted", "credential-1" }
                                td { class: "p-2 text-lightning-cyan", "enabled" }
                                td {
                                    class: "p-2",
                                    button { class: ACTION_BUTTON_CLASS, "Disable" }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: "border border-terminal-border bg-terminal-surface p-4",
                h3 {
                    class: "mb-3 mt-0 text-sm font-semibold text-terminal-text",
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

/// Builds the browser bridge script with stable endpoint and terminal defaults.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn terminal_bridge_script() -> String {
    format!(
        r##"
(() => {{
  const mount = document.getElementById("{mount_id}");
  const status = document.getElementById("sunbolt-terminal-status");
  const closeButton = document.getElementById("sunbolt-terminal-close");
  const mfaButton = document.getElementById("sunbolt-terminal-mfa");
  const reconnectButton = document.getElementById("sunbolt-terminal-reconnect");
  const nodeInput = document.getElementById("{node_input_id}");
  if (!mount || mount.dataset.sunboltTerminalReady === "true") {{
    return;
  }}
  mount.dataset.sunboltTerminalReady = "true";

  let sessionId = null;
  let cols = {cols};
  let rows = {rows};
  let socket = null;
  let terminal = null;
  let fallbackInput = null;
  let fallbackOutput = null;

  const setStatus = (label, state) => {{
    if (!status) {{
      return;
    }}
    status.textContent = label;
    const classes = {{
      connecting: "{status_connecting_class}",
      connected: "{status_connected_class}",
      error: "{status_error_class}",
      closed: "{status_closed_class}"
    }};
    status.className = classes[state] || "{status_base_class}";
    if (closeButton) {{
      closeButton.disabled = state === "closed";
    }}
    if (reconnectButton) {{
      reconnectButton.disabled = true;
    }}
  }};

  const writeOutput = (data) => {{
    if (terminal) {{
      terminal.write(data);
      return;
    }}
    if (fallbackOutput) {{
      fallbackOutput.textContent += data;
      fallbackOutput.scrollTop = fallbackOutput.scrollHeight;
    }}
  }};

  const send = (message) => {{
    if (socket && socket.readyState === WebSocket.OPEN) {{
      socket.send(JSON.stringify(message));
    }}
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

  const connect = () => {{
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${{protocol}}//${{window.location.host}}{endpoint}`;
    socket = new WebSocket(url);

    socket.addEventListener("open", () => {{
      setStatus("Connected", "connected");
      resize();
      send({{
        type: "start",
        node_id: nodeInput && nodeInput.value.trim() ? nodeInput.value.trim() : null,
        initial_size: {{ cols, rows }}
      }});
    }});

    socket.addEventListener("message", (event) => {{
      const message = JSON.parse(event.data);
      if (message.type === "started") {{
        sessionId = message.session_id;
        setStatus("Active", "connected");
      }} else if (message.type === "output") {{
        writeOutput(message.data);
      }} else if (message.type === "error") {{
        setStatus("Error", "error");
        writeOutput(`\r\n${{message.error.message}}\r\n`);
      }} else if (message.type === "exited") {{
        setStatus("Closed", "closed");
      }}
    }});

    socket.addEventListener("close", () => {{
      setStatus("Closed", "closed");
    }});

    socket.addEventListener("error", () => {{
      setStatus("Error", "error");
    }});
  }};

  const sendInput = (data) => {{
    if (!sessionId) {{
      return;
    }}
    send({{ type: "input", session_id: sessionId, data }});
  }};

  const closeTerminal = () => {{
    if (sessionId) {{
      send({{ type: "close", session_id: sessionId }});
    }}
    if (socket) {{
      socket.close();
    }}
    setStatus("Closed", "closed");
  }};

  const completeStepUpMfa = async () => {{
    setStatus("MFA", "connecting");
    const response = await fetch("{step_up_mfa_endpoint}", {{
      method: "POST",
      headers: {{ "content-type": "application/json" }},
      body: JSON.stringify({{ factor_type: "totp" }})
    }});
    if (!response.ok) {{
      setStatus("MFA Required", "error");
      return;
    }}
    setStatus("Connecting", "connecting");
    connect();
  }};

  if (window.Terminal) {{
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
    terminal.onData(sendInput);
  }} else {{
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

  const observer = new ResizeObserver(resize);
  observer.observe(mount);
  if (closeButton) {{
    closeButton.addEventListener("click", closeTerminal);
  }}
  if (mfaButton) {{
    mfaButton.addEventListener("click", completeStepUpMfa);
  }}
  window.addEventListener("beforeunload", () => {{
    if (sessionId) {{
      send({{ type: "close", session_id: sessionId }});
    }}
  }});
  connect();
}})();
"##,
        mount_id = TERMINAL_MOUNT_ID,
        node_input_id = TERMINAL_NODE_INPUT_ID,
        endpoint = TERMINAL_WS_ENDPOINT,
        step_up_mfa_endpoint = STEP_UP_MFA_ENDPOINT,
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
        app_title, terminal_bridge_script, STEP_UP_MFA_ENDPOINT, TERMINAL_MOUNT_ID,
        TERMINAL_NODE_INPUT_ID, TERMINAL_WS_ENDPOINT,
    };

    #[test]
    fn app_title_uses_product_name() {
        assert_eq!(app_title(), "Sunbolt");
    }

    #[test]
    fn terminal_bridge_uses_terminal_websocket_endpoint() {
        let script = terminal_bridge_script();

        assert!(script.contains(TERMINAL_WS_ENDPOINT));
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
}
