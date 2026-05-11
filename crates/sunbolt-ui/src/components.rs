pub mod layout {
    pub const SHELL: &str = "sunbolt-shell";
    pub const APP_GRID: &str = "sunbolt-app-grid";
    pub const TOPBAR: &str = "sunbolt-topbar";
    pub const BRAND: &str = "sunbolt-brand";
    pub const BRAND_MARK: &str = "sunbolt-brand-mark";
    pub const NAV: &str = "sunbolt-nav";
    pub const PAGE: &str = "sunbolt-page";
    pub const CARD: &str = "sunbolt-card";
}

pub mod table_list {
    pub const TABLE_WRAP: &str = "sunbolt-table-wrap";
    pub const TABLE: &str = "sunbolt-table";
    pub const DENSE_LIST: &str = "sunbolt-dense-list";
    pub const DENSE_ROW: &str = "sunbolt-dense-row";
}

pub mod form {
    pub const TEXT_INPUT: &str = "sunbolt-input";
    pub const FIELD_GROUP: &str = "sunbolt-field-group";
}

pub mod dialog {
    pub const MODAL: &str = "sunbolt-modal";
    pub const MODAL_PANEL: &str = "sunbolt-modal-panel";
    pub const MODAL_ACTIONS: &str = "sunbolt-modal-actions";
}

pub mod bottom_sheet {
    pub const SHEET: &str = "sunbolt-bottom-sheet";
    pub const SHEET_PANEL: &str = "sunbolt-bottom-sheet-panel";
    pub const SHEET_ACTIONS: &str = "sunbolt-bottom-sheet-actions";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Navigation { active: bool },
}

#[must_use]
pub const fn button_class(variant: ButtonVariant) -> &'static str {
    match variant {
        ButtonVariant::Primary => "sunbolt-button sunbolt-button-primary",
        ButtonVariant::Secondary => "sunbolt-button sunbolt-button-secondary",
        ButtonVariant::Danger => "sunbolt-button sunbolt-button-danger",
        ButtonVariant::Navigation { active: true } => {
            "sunbolt-nav-button sunbolt-nav-button-active"
        }
        ButtonVariant::Navigation { active: false } => "sunbolt-nav-button",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusTone {
    Base,
    Connecting,
    Connected,
    Degraded,
    Error,
    Closed,
}

#[must_use]
pub const fn status_badge_class(tone: StatusTone) -> &'static str {
    match tone {
        StatusTone::Base => "sunbolt-status border-terminal-border text-terminal-muted",
        StatusTone::Connecting | StatusTone::Degraded => {
            "sunbolt-status border-sun-amber/70 bg-sun-amber/10 text-sun-amber"
        }
        StatusTone::Connected => {
            "sunbolt-status border-lightning-cyan/70 bg-lightning-cyan/10 text-lightning-cyan"
        }
        StatusTone::Error => {
            "sunbolt-status border-warm-orange/70 bg-warm-orange/10 text-warm-orange"
        }
        StatusTone::Closed => {
            "sunbolt-status border-terminal-border bg-terminal-bg/80 text-terminal-muted"
        }
    }
}
