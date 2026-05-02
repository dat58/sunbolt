use dioxus::prelude::*;
use sunbolt_protocol::TerminalSize;

/// DOM id used by the browser terminal bridge.
pub const TERMINAL_MOUNT_ID: &str = "sunbolt-terminal";

/// WebSocket endpoint used by the terminal UI.
pub const TERMINAL_WS_ENDPOINT: &str = "/terminal/ws";

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
const FALLBACK_OUTPUT_CLASS: &str =
    "box-border h-[calc(100%-72px)] m-0 overflow-auto whitespace-pre-wrap font-mono text-sm";
const FALLBACK_INPUT_CLASS: &str =
    "mt-3 h-14 w-full resize-none box-border border border-terminal-border bg-terminal-surface font-mono text-sm text-terminal-text";

/// Returns the display title for the web UI shell.
#[must_use]
pub fn app_title() -> String {
    sunbolt_common::product_name().to_owned()
}

/// Root Dioxus app for the Sunbolt web UI.
#[component]
pub fn App() -> Element {
    rsx! {
        TerminalPage {}
    }
}

/// First local terminal page.
#[component]
pub fn TerminalPage() -> Element {
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
                    div {
                        id: "sunbolt-terminal-status",
                        class: STATUS_CONNECTING_CLASS,
                        "Connecting"
                    }
                }
                div {
                    id: TERMINAL_MOUNT_ID,
                    class: "min-h-0 overflow-hidden p-3 [&_.xterm]:h-full",
                    tabindex: "0",
                    "Terminal loading"
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
            script {
                dangerous_inner_html: terminal_bridge_script()
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
        node_id: null,
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
  window.addEventListener("beforeunload", () => {{
    if (sessionId) {{
      send({{ type: "close", session_id: sessionId }});
    }}
  }});
  connect();
}})();
"##,
        mount_id = TERMINAL_MOUNT_ID,
        endpoint = TERMINAL_WS_ENDPOINT,
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
    use super::{app_title, terminal_bridge_script, TERMINAL_MOUNT_ID, TERMINAL_WS_ENDPOINT};

    #[test]
    fn app_title_uses_product_name() {
        assert_eq!(app_title(), "Sunbolt");
    }

    #[test]
    fn terminal_bridge_uses_terminal_websocket_endpoint() {
        let script = terminal_bridge_script();

        assert!(script.contains(TERMINAL_WS_ENDPOINT));
        assert!(script.contains(TERMINAL_MOUNT_ID));
        assert!(script.contains(r#"type: "start""#));
        assert!(script.contains(r#"type: "input""#));
        assert!(script.contains(r#"type: "resize""#));
        assert!(script.contains(r#"type: "close""#));
        assert!(script.contains("border-lightning-cyan"));
    }
}
