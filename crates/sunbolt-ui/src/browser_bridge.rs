use crate::api_client::{
    AUTH_LOGIN_ENDPOINT, AUTH_ME_ENDPOINT, AUTH_TERMINAL_ACCESS_ENDPOINT,
    CONTROL_PLANE_URL_CONFIG_GLOBAL, STEP_UP_MFA_ENDPOINT, TERMINAL_ACTIVE_SESSIONS_ENDPOINT,
    TERMINAL_DETACHED_SESSIONS_ENDPOINT, TERMINAL_SESSION_TERMINATE_PREFIX,
    TERMINAL_WS_CONFIG_GLOBAL, TERMINAL_WS_ENDPOINT,
};
use crate::components::{status_badge_class, StatusTone};
use crate::terminal_workspace::{DEFAULT_TERMINAL_SIZE, TERMINAL_MOUNT_ID, TERMINAL_NODE_INPUT_ID};

pub const XTERM_SCRIPT_URL: &str =
    "https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js";
pub const XTERM_STYLESHEET_URL: &str =
    "https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.min.css";

const STATUS_BASE_CLASS: &str = status_badge_class(StatusTone::Base);
const STATUS_CONNECTING_CLASS: &str = status_badge_class(StatusTone::Connecting);
const STATUS_CONNECTED_CLASS: &str = status_badge_class(StatusTone::Connected);
const STATUS_DEGRADED_CLASS: &str = status_badge_class(StatusTone::Degraded);
const STATUS_ERROR_CLASS: &str = status_badge_class(StatusTone::Error);
const STATUS_CLOSED_CLASS: &str = status_badge_class(StatusTone::Closed);
const FALLBACK_OUTPUT_CLASS: &str = "sunbolt-fallback-output";
const FALLBACK_INPUT_CLASS: &str = "sunbolt-fallback-input";

/// Builds the browser bridge script with stable endpoint and terminal defaults.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn terminal_bridge_script() -> String {
    format!(
        r##"
(async () => {{
  const mount = document.getElementById("{mount_id}");
  const status = document.getElementById("sunbolt-terminal-status");
  const errorDisplay = document.getElementById("sunbolt-terminal-error");
  const authPanel = document.getElementById("sunbolt-terminal-auth");
  const emailInput = document.getElementById("sunbolt-terminal-email");
  const passwordInput = document.getElementById("sunbolt-terminal-password");
  const loginButton = document.getElementById("sunbolt-terminal-login");
  const closeButton = document.getElementById("sunbolt-terminal-close");
  const newButton = document.getElementById("sunbolt-terminal-new");
  const detachButton = document.getElementById("sunbolt-terminal-detach");
  const closeTabButton = document.getElementById("sunbolt-terminal-close-tab");
  const mfaButton = document.getElementById("sunbolt-terminal-mfa");
  const reconnectButton = document.getElementById("sunbolt-terminal-reconnect");
  const retryButton = document.getElementById("sunbolt-terminal-retry");
  const mobileCtrlButton = document.getElementById("sunbolt-mobile-key-ctrl");
  const mobileEscButton = document.getElementById("sunbolt-mobile-key-esc");
  const mobileTabButton = document.getElementById("sunbolt-mobile-key-tab");
  const mobileUpButton = document.getElementById("sunbolt-mobile-key-up");
  const mobileLeftButton = document.getElementById("sunbolt-mobile-key-left");
  const mobileDownButton = document.getElementById("sunbolt-mobile-key-down");
  const mobileRightButton = document.getElementById("sunbolt-mobile-key-right");
  const mobilePasteButton = document.getElementById("sunbolt-mobile-key-paste");
  const mobileReconnectButton = document.getElementById("sunbolt-mobile-action-reconnect");
  const mobileDetachButton = document.getElementById("sunbolt-mobile-action-detach");
  const mobileTerminateButton = document.getElementById("sunbolt-mobile-action-terminate");
  const nodeInput = document.getElementById("{node_input_id}");
  const tabList = document.getElementById("sunbolt-terminal-tabs");
  const detachedList = document.getElementById("sunbolt-terminal-detached-sessions");
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
  let mobileCtrlActive = false;
  const workspaceStorageKey = "sunbolt.terminal.workspace.v1";
  const activeSessions = new Map();

  const terminalData = (data) => {{
    if (typeof data !== "string") {{
      return "";
    }}
    return data.replace(/\u0000/g, "");
  }};

  const stripTerminalControls = (data) => terminalData(data)
    .replace(/\x1B\][^\x07]*(?:\x07|\x1B\\)/g, "")
    .replace(/\x1B\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\x1B[=>]/g, "");

  const loadXterm = () => new Promise((resolve) => {{
    if (window.Terminal) {{
      resolve(true);
      return;
    }}

    const existingScript = Array.from(document.scripts)
      .find((script) => script.src === "{xterm_script_url}");
    const script = existingScript || document.createElement("script");
    let settled = false;
    const finish = (loaded) => {{
      if (settled) {{
        return;
      }}
      settled = true;
      resolve(Boolean(loaded && window.Terminal));
    }};

    script.addEventListener("load", () => finish(true), {{ once: true }});
    script.addEventListener("error", () => finish(false), {{ once: true }});
    if (!existingScript) {{
      script.src = "{xterm_script_url}";
      script.async = true;
      document.head.appendChild(script);
    }}
    window.setTimeout(() => finish(Boolean(window.Terminal)), 3000);
  }});

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

  const csrfHeaders = (headers = {{}}) => ({{
    ...headers,
    "x-sunbolt-csrf": "1"
  }});

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
    for (const button of [detachButton, closeTabButton]) {{
      if (button) {{
        button.disabled = !authenticated || !sessionId || currentStatusState === "idle";
      }}
    }}
    if (newButton) {{
      newButton.disabled = !authenticated || loginBusy || currentStatusState === "connecting";
    }}
    if (reconnectButton) {{
      reconnectButton.disabled = !(
        currentStatusState === "disconnected" && sessionId && reconnectToken
      );
    }}
    if (mobileReconnectButton) {{
      mobileReconnectButton.disabled = !(
        currentStatusState === "disconnected" && sessionId && reconnectToken
      );
    }}
    if (mobileDetachButton) {{
      mobileDetachButton.disabled = !authenticated || !sessionId || currentStatusState === "idle";
    }}
    if (mobileTerminateButton) {{
      mobileTerminateButton.disabled = !authenticated
        || currentStatusState === "idle"
        || currentStatusState === "closed"
        || currentStatusState === "disconnected";
    }}
    if (retryButton) {{
      retryButton.disabled = loginBusy
        || currentStatusState === "connecting"
        || currentStatusState === "connected"
        || currentStatusState === "degraded";
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
      degraded: "{status_degraded_class}",
      error: "{status_error_class}",
      disconnected: "{status_closed_class}",
      closed: "{status_closed_class}"
    }};
    status.className = classes[state] || "{status_base_class}";
    syncControls();
  }};

  const transportStatusLabel = (transportStatus) => {{
    if (!transportStatus || !transportStatus.degraded) {{
      return null;
    }}
    if (transportStatus.kind === "long_poll_https") {{
      return "Degraded: Long Poll";
    }}
    return "Degraded Transport";
  }};

  const reportTransportStatus = (transportStatus) => {{
    const label = transportStatusLabel(transportStatus);
    if (!label) {{
      return false;
    }}
    setStatus(label, "degraded");
    if (transportStatus.message) {{
      setError(transportStatus.message);
    }}
    return true;
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
      fallbackOutput.textContent += stripTerminalControls(safeData);
      fallbackOutput.scrollTop = fallbackOutput.scrollHeight;
    }}
  }};

  const terminalLabel = (record) => {{
    if (!record) {{
      return "Terminal";
    }}
    const node = record.node_id || "local";
    return `${{node}}:${{String(record.session_id || "").slice(-6)}}`;
  }};

  const persistWorkspace = () => {{
    const payload = {{
      active_session_id: sessionId,
      sessions: Array.from(activeSessions.values())
    }};
    try {{
      window.sessionStorage.setItem(workspaceStorageKey, JSON.stringify(payload));
    }} catch (_error) {{}}
  }};

  const loadWorkspace = () => {{
    try {{
      const payload = JSON.parse(window.sessionStorage.getItem(workspaceStorageKey) || "{{}}");
      for (const record of payload.sessions || []) {{
        if (record && record.session_id) {{
          activeSessions.set(record.session_id, record);
        }}
      }}
      return payload && payload.active_session_id ? payload.active_session_id : null;
    }} catch (_error) {{
      return null;
    }}
  }};

  const rememberSession = (message, state = "active") => {{
    if (!message || !message.session_id) {{
      return;
    }}
    activeSessions.set(message.session_id, {{
      session_id: message.session_id,
      node_id: message.node_id || null,
      reconnect_token: message.reconnect_token || reconnectToken,
      state,
      transport_status: message.transport_status || null
    }});
    persistWorkspace();
    renderTabs();
  }};

  const removeSession = (id) => {{
    if (!id) {{
      return;
    }}
    activeSessions.delete(id);
    if (sessionId === id) {{
      sessionId = null;
      reconnectToken = null;
    }}
    persistWorkspace();
    renderTabs();
  }};

  const renderTabs = () => {{
    if (!tabList) {{
      return;
    }}
    tabList.innerHTML = "";
    for (const record of activeSessions.values()) {{
      const button = document.createElement("button");
      button.type = "button";
      button.className = record.session_id === sessionId
        ? "sunbolt-tab sunbolt-tab-active"
        : "sunbolt-tab";
      button.textContent = terminalLabel(record);
      button.addEventListener("click", () => {{
        sessionId = record.session_id;
        reconnectToken = record.reconnect_token || null;
        setStatus("Reconnecting", "connecting");
        connect(true);
      }});
      tabList.append(button);
    }}
  }};

  const renderDetachedSessions = (sessions) => {{
    if (!detachedList) {{
      return;
    }}
    detachedList.innerHTML = "";
    for (const record of sessions || []) {{
      const row = document.createElement("button");
      row.type = "button";
      row.className = "sunbolt-detached-session";
      row.textContent = `${{terminalLabel(record)}} detached`;
      row.addEventListener("click", () => {{
        const stored = activeSessions.get(record.session_id);
        sessionId = record.session_id;
        reconnectToken = stored && stored.reconnect_token ? stored.reconnect_token : reconnectToken;
        if (!reconnectToken) {{
          setError("This detached session requires a reconnect token from the active browser workspace.");
          return;
        }}
        connect(true);
      }});
      detachedList.append(row);
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

  const refreshSessionLists = async () => {{
    if (!authenticated) {{
      return;
    }}
    try {{
      const [activeResponse, detachedResponse] = await Promise.all([
        fetch(httpEndpointUrl("{terminal_active_sessions_endpoint}"), {{ credentials: "include" }}),
        fetch(httpEndpointUrl("{terminal_detached_sessions_endpoint}"), {{ credentials: "include" }})
      ]);
      if (activeResponse.ok) {{
        const payload = await activeResponse.json();
        for (const record of payload.sessions || []) {{
          const existing = activeSessions.get(record.session_id) || {{}};
          activeSessions.set(record.session_id, {{ ...existing, ...record }});
        }}
      }}
      if (detachedResponse.ok) {{
        const payload = await detachedResponse.json();
        renderDetachedSessions(payload.sessions || []);
      }}
      persistWorkspace();
      renderTabs();
    }} catch (_error) {{}}
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
        refreshSessionLists();
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
        rememberSession(message, "active");
        if (!reportTransportStatus(message.transport_status)) {{
          setStatus("Active", "connected");
        }}
      }} else if (message.type === "reattached") {{
        sessionId = message.session_id;
        reconnectToken = message.reconnect_token || reconnectToken;
        rememberSession(message, "active");
        if (!reportTransportStatus(message.transport_status)) {{
          setStatus("Active", "connected");
        }}
      }} else if (message.type === "detached") {{
        const stored = activeSessions.get(message.session_id) || {{ session_id: message.session_id }};
        activeSessions.set(message.session_id, {{ ...stored, state: "detached", reconnect_token: reconnectToken }});
        persistWorkspace();
        renderTabs();
        refreshSessionLists();
        setStatus("Detached", "disconnected");
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
        removeSession(message.session_id);
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
        headers: csrfHeaders({{ "content-type": "application/json" }}),
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
    const safeData = terminalData(data);
    if (mobileCtrlActive && /^[a-zA-Z]$/.test(safeData)) {{
      setMobileCtrl(false);
      send({{
        type: "input",
        session_id: sessionId,
        data: String.fromCharCode(safeData.toUpperCase().charCodeAt(0) - 64)
      }});
      return;
    }}
    send({{ type: "input", session_id: sessionId, data: safeData }});
  }};

  const setMobileCtrl = (active) => {{
    mobileCtrlActive = active;
    if (mobileCtrlButton) {{
      mobileCtrlButton.dataset.active = active ? "true" : "false";
      mobileCtrlButton.setAttribute("aria-pressed", active ? "true" : "false");
    }}
  }};

  const sendMobileKey = (data) => {{
    sendInput(data);
    setMobileCtrl(false);
    if (terminal) {{
      terminal.focus();
    }} else if (fallbackInput) {{
      fallbackInput.focus();
    }}
  }};

  const sendMobilePaste = async () => {{
    try {{
      if (!navigator.clipboard || typeof navigator.clipboard.readText !== "function") {{
        setError("Clipboard paste is not available in this browser context.");
        return;
      }}
      const text = await navigator.clipboard.readText();
      if (text) {{
        sendMobileKey(text);
      }}
    }} catch (_error) {{
      setError("Unable to read from the clipboard.");
    }}
  }};

  const closeTerminal = () => {{
    if (sessionId) {{
      send({{ type: "terminate", session_id: sessionId }});
      fetch(httpEndpointUrl(`{terminal_session_terminate_prefix}/${{sessionId}}/terminate`), {{
        method: "POST",
        credentials: "include",
        headers: csrfHeaders()
      }}).catch(() => {{}});
    }}
    removeSession(sessionId);
    reconnectToken = null;
    if (socket) {{
      socket.close();
    }}
    setStatus("Closed", "closed");
  }};

  const detachTerminal = () => {{
    if (!sessionId) {{
      return;
    }}
    send({{ type: "detach", session_id: sessionId }});
    const stored = activeSessions.get(sessionId) || {{ session_id: sessionId }};
    activeSessions.set(sessionId, {{ ...stored, state: "detached", reconnect_token: reconnectToken }});
    persistWorkspace();
    renderTabs();
    if (socket) {{
      socket.close();
    }}
    setStatus("Detached", "disconnected");
  }};

  const closeUiTab = () => {{
    const closedSessionId = sessionId;
    detachTerminal();
    removeSession(closedSessionId);
  }};

  const openNewTerminal = () => {{
    if (socket) {{
      socket.close();
    }}
    sessionId = null;
    reconnectToken = null;
    if (terminal) {{
      terminal.clear();
    }}
    connect(false);
  }};

  const cleanupTerminal = () => {{
    if (cleanedUp) {{
      return;
    }}
    cleanedUp = true;
    if (sessionId && reconnectToken) {{
      send({{ type: "detach", session_id: sessionId }});
    }} else if (sessionId) {{
      send({{ type: "detach", session_id: sessionId }});
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
        headers: csrfHeaders({{ "content-type": "application/json" }}),
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

  if (await loadXterm()) {{
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
    fallbackInput.placeholder = "Type a command and press Enter";
    fallbackInput.spellcheck = false;
    fallbackInput.addEventListener("keydown", (event) => {{
      const keyMap = {{
        ArrowUp: "\x1b[A",
        ArrowDown: "\x1b[B",
        ArrowRight: "\x1b[C",
        ArrowLeft: "\x1b[D",
        Home: "\x1b[H",
        End: "\x1b[F",
        Delete: "\x1b[3~",
        PageUp: "\x1b[5~",
        PageDown: "\x1b[6~"
      }};
      if (event.ctrlKey && event.key.toLowerCase() === "c") {{
        event.preventDefault();
        sendInput("\x03");
        return;
      }}
      if (event.key === "Tab") {{
        event.preventDefault();
        sendInput("\t");
        return;
      }}
      if (keyMap[event.key]) {{
        event.preventDefault();
        sendInput(keyMap[event.key]);
        return;
      }}
      if (event.key === "Enter" && !event.shiftKey) {{
        event.preventDefault();
        sendInput(`${{fallbackInput.value}}\r`);
        fallbackInput.value = "";
      }}
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
  if (newButton) {{
    newButton.addEventListener("click", openNewTerminal);
  }}
  if (detachButton) {{
    detachButton.addEventListener("click", detachTerminal);
  }}
  if (closeTabButton) {{
    closeTabButton.addEventListener("click", closeUiTab);
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
  if (mobileCtrlButton) {{
    mobileCtrlButton.addEventListener("click", () => {{
      setMobileCtrl(!mobileCtrlActive);
    }});
  }}
  if (mobileEscButton) {{
    mobileEscButton.addEventListener("click", () => sendMobileKey("\x1b"));
  }}
  if (mobileTabButton) {{
    mobileTabButton.addEventListener("click", () => sendMobileKey("\t"));
  }}
  if (mobileUpButton) {{
    mobileUpButton.addEventListener("click", () => sendMobileKey("\x1b[A"));
  }}
  if (mobileLeftButton) {{
    mobileLeftButton.addEventListener("click", () => sendMobileKey("\x1b[D"));
  }}
  if (mobileDownButton) {{
    mobileDownButton.addEventListener("click", () => sendMobileKey("\x1b[B"));
  }}
  if (mobileRightButton) {{
    mobileRightButton.addEventListener("click", () => sendMobileKey("\x1b[C"));
  }}
  if (mobilePasteButton) {{
    mobilePasteButton.addEventListener("click", sendMobilePaste);
  }}
  if (mobileReconnectButton) {{
    mobileReconnectButton.addEventListener("click", () => {{
      if (!sessionId || !reconnectToken) {{
        return;
      }}
      setStatus("Reconnecting", "connecting");
      connect(true);
    }});
  }}
  if (mobileDetachButton) {{
    mobileDetachButton.addEventListener("click", detachTerminal);
  }}
  if (mobileTerminateButton) {{
    mobileTerminateButton.addEventListener("click", closeTerminal);
  }}
  window.addEventListener("beforeunload", cleanupTerminal);
  window.addEventListener("pagehide", cleanupTerminal);
  setStatus("Idle", "idle");
  setAuthVisible(false);
  const storedSessionId = loadWorkspace();
  renderTabs();
  if (storedSessionId && activeSessions.has(storedSessionId)) {{
    const stored = activeSessions.get(storedSessionId);
    sessionId = stored.session_id;
    reconnectToken = stored.reconnect_token || null;
    if (reconnectToken) {{
      connect(true);
    }} else {{
      connect();
    }}
  }} else {{
    connect();
  }}
}})();
"##,
        mount_id = TERMINAL_MOUNT_ID,
        node_input_id = TERMINAL_NODE_INPUT_ID,
        endpoint = TERMINAL_WS_ENDPOINT,
        auth_login_endpoint = AUTH_LOGIN_ENDPOINT,
        auth_me_endpoint = AUTH_ME_ENDPOINT,
        auth_terminal_access_endpoint = AUTH_TERMINAL_ACCESS_ENDPOINT,
        terminal_active_sessions_endpoint = TERMINAL_ACTIVE_SESSIONS_ENDPOINT,
        terminal_detached_sessions_endpoint = TERMINAL_DETACHED_SESSIONS_ENDPOINT,
        terminal_session_terminate_prefix = TERMINAL_SESSION_TERMINATE_PREFIX,
        step_up_mfa_endpoint = STEP_UP_MFA_ENDPOINT,
        control_plane_config_global = CONTROL_PLANE_URL_CONFIG_GLOBAL,
        ws_config_global = TERMINAL_WS_CONFIG_GLOBAL,
        xterm_script_url = XTERM_SCRIPT_URL,
        cols = DEFAULT_TERMINAL_SIZE.cols,
        rows = DEFAULT_TERMINAL_SIZE.rows,
        status_base_class = STATUS_BASE_CLASS,
        status_connecting_class = STATUS_CONNECTING_CLASS,
        status_connected_class = STATUS_CONNECTED_CLASS,
        status_degraded_class = STATUS_DEGRADED_CLASS,
        status_error_class = STATUS_ERROR_CLASS,
        status_closed_class = STATUS_CLOSED_CLASS,
        fallback_output_class = FALLBACK_OUTPUT_CLASS,
        fallback_input_class = FALLBACK_INPUT_CLASS,
    )
}
