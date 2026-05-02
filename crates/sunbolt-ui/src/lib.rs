use dioxus::prelude::*;
use sunbolt_protocol::TerminalSize;

/// DOM id used by the browser terminal bridge.
pub const TERMINAL_MOUNT_ID: &str = "sunbolt-terminal";

/// WebSocket endpoint used by the terminal UI.
pub const TERMINAL_WS_ENDPOINT: &str = "/terminal/ws";

const DEFAULT_TERMINAL_SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };

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
            class: "sunbolt-terminal-page",
            style { dangerous_inner_html: TERMINAL_PAGE_CSS }
            section {
                class: "terminal-shell",
                header {
                    class: "terminal-toolbar",
                    h1 { "Sunbolt" }
                    div {
                        id: "sunbolt-terminal-status",
                        class: "terminal-status terminal-status-connecting",
                        "Connecting"
                    }
                }
                div {
                    id: TERMINAL_MOUNT_ID,
                    class: "terminal-mount",
                    tabindex: "0",
                    "Terminal loading"
                }
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
    status.className = `terminal-status terminal-status-${{state}}`;
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
    fallbackOutput.className = "terminal-fallback-output";
    fallbackInput = document.createElement("textarea");
    fallbackInput.className = "terminal-fallback-input";
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
    )
}

const TERMINAL_PAGE_CSS: &str = r#"
html,
body,
#main {
  height: 100%;
  margin: 0;
}

.sunbolt-terminal-page {
  min-height: 100vh;
  background: #09090B;
  color: #FAFAFA;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

.terminal-shell {
  min-height: 100vh;
  display: grid;
  grid-template-rows: 48px minmax(0, 1fr);
}

.terminal-toolbar {
  align-items: center;
  border-bottom: 1px solid #27272A;
  background: #18181B;
  display: flex;
  justify-content: space-between;
  padding: 0 16px;
}

.terminal-toolbar h1 {
  color: #FBBF24;
  font-size: 15px;
  font-weight: 700;
  margin: 0;
}

.terminal-status {
  align-items: center;
  border: 1px solid #27272A;
  border-radius: 999px;
  color: #A1A1AA;
  display: inline-flex;
  font-size: 12px;
  height: 24px;
  padding: 0 10px;
}

.terminal-status-connected {
  border-color: #22D3EE;
  color: #22D3EE;
}

.terminal-status-error {
  border-color: #F59E0B;
  color: #F59E0B;
}

.terminal-status-closed {
  color: #A1A1AA;
}

.terminal-mount {
  min-height: 0;
  overflow: hidden;
  padding: 12px;
}

.terminal-mount .xterm {
  height: 100%;
}

.terminal-fallback-output {
  box-sizing: border-box;
  height: calc(100% - 72px);
  margin: 0;
  overflow: auto;
  white-space: pre-wrap;
}

.terminal-fallback-input {
  background: #18181B;
  border: 1px solid #27272A;
  box-sizing: border-box;
  color: #FAFAFA;
  font: 14px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  height: 56px;
  margin-top: 12px;
  resize: none;
  width: 100%;
}
"#;

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
    }
}
