use dioxus::prelude::*;

use crate::api_client::control_plane_config_script;
use crate::browser_bridge::terminal_bridge_script;
use crate::components::{button_class, layout, ButtonVariant};
use crate::pages::{
    AccessHistoryPage, AdminPage, AuditLogPage, DashboardPage, NodesPage, SecurityPage,
    TerminalPageBody,
};
use crate::{XTERM_SCRIPT_URL, XTERM_STYLESHEET_URL};

/// Returns the display title for the web UI shell.
#[must_use]
pub fn app_title() -> String {
    sunbolt_common::product_name().to_owned()
}

/// Root Dioxus app for the Sunbolt web UI.
#[component]
pub fn App() -> Element {
    let mut page = use_signal(|| ShellPage::Dashboard);

    rsx! {
        main {
            class: layout::SHELL,
            section {
                class: layout::APP_GRID,
                header {
                    class: layout::TOPBAR,
                    div {
                        class: layout::BRAND,
                        div {
                            class: layout::BRAND_MARK,
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
                        class: layout::NAV,
                        "aria-label": "Primary navigation",
                        button {
                            class: nav_class(page(), ShellPage::Dashboard),
                            onclick: move |_| page.set(ShellPage::Dashboard),
                            "Dashboard"
                        }
                        button {
                            class: nav_class(page(), ShellPage::Terminal),
                            onclick: move |_| page.set(ShellPage::Terminal),
                            "Terminal"
                        }
                        button {
                            class: nav_class(page(), ShellPage::AccessHistory),
                            onclick: move |_| page.set(ShellPage::AccessHistory),
                            "Access History"
                        }
                        button {
                            class: nav_class(page(), ShellPage::Nodes),
                            onclick: move |_| page.set(ShellPage::Nodes),
                            "Nodes"
                        }
                        button {
                            class: nav_class(page(), ShellPage::AuditLogs),
                            onclick: move |_| page.set(ShellPage::AuditLogs),
                            "Audit Logs"
                        }
                        button {
                            class: nav_class(page(), ShellPage::Security),
                            onclick: move |_| page.set(ShellPage::Security),
                            "Security"
                        }
                        button {
                            class: nav_class(page(), ShellPage::Admin),
                            onclick: move |_| page.set(ShellPage::Admin),
                            "Admin"
                        }
                    }
                }
                match page() {
                    ShellPage::Dashboard => rsx! { DashboardPage {} },
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
            link {
                rel: "stylesheet",
                href: asset!("/assets/sunbolt-terminal-workspace.css")
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
    Dashboard,
    Terminal,
    AccessHistory,
    Nodes,
    AuditLogs,
    Security,
    Admin,
}

fn nav_class(current: ShellPage, target: ShellPage) -> &'static str {
    button_class(ButtonVariant::Navigation {
        active: current == target,
    })
}
