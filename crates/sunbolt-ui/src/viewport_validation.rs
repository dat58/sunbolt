#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequiredViewport {
    pub label: &'static str,
    pub width: u16,
    pub height: u16,
    pub class: ViewportClass,
}

impl RequiredViewport {
    #[must_use]
    pub const fn dimensions(self) -> (u16, u16) {
        (self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewportClass {
    Mobile,
    TabletPortrait,
    TabletLandscape,
    Laptop,
    Desktop,
}

impl ViewportClass {
    #[must_use]
    pub const fn from_size(width: u16, height: u16) -> Self {
        if width < 768 {
            Self::Mobile
        } else if width < 1024 {
            Self::TabletPortrait
        } else if width < 1280 && height < width {
            Self::TabletLandscape
        } else if width < 1536 {
            Self::Laptop
        } else {
            Self::Desktop
        }
    }

    #[must_use]
    pub const fn expected_terminal_layout(self) -> TerminalLayoutExpectation {
        match self {
            Self::Mobile => TerminalLayoutExpectation::MobileTerminalFirst,
            Self::TabletPortrait | Self::TabletLandscape => {
                TerminalLayoutExpectation::TabletTwoPane
            }
            Self::Laptop | Self::Desktop => TerminalLayoutExpectation::DenseControlPlane,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalLayoutExpectation {
    MobileTerminalFirst,
    TabletTwoPane,
    DenseControlPlane,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewportValidationCheck {
    Navigation,
    TerminalUsability,
    SessionSwitching,
    NoTextOrControlOverlap,
    LoginFlow,
    MfaFlow,
    TerminalLifecycleSemantics,
}

pub const REQUIRED_VIEWPORT_VALIDATION_CHECKS: [ViewportValidationCheck; 7] = [
    ViewportValidationCheck::Navigation,
    ViewportValidationCheck::TerminalUsability,
    ViewportValidationCheck::SessionSwitching,
    ViewportValidationCheck::NoTextOrControlOverlap,
    ViewportValidationCheck::LoginFlow,
    ViewportValidationCheck::MfaFlow,
    ViewportValidationCheck::TerminalLifecycleSemantics,
];

pub const UI_VALIDATION_ARTIFACT: &str = "cargo test -p sunbolt-ui viewport_validation";

pub const REQUIRED_VIEWPORTS: [RequiredViewport; 5] = [
    RequiredViewport {
        label: "iPhone 11 Pro",
        width: 375,
        height: 812,
        class: ViewportClass::Mobile,
    },
    RequiredViewport {
        label: "iPad 11 Pro portrait",
        width: 834,
        height: 1194,
        class: ViewportClass::TabletPortrait,
    },
    RequiredViewport {
        label: "iPad 11 Pro landscape",
        width: 1194,
        height: 834,
        class: ViewportClass::TabletLandscape,
    },
    RequiredViewport {
        label: "Laptop",
        width: 1366,
        height: 768,
        class: ViewportClass::Laptop,
    },
    RequiredViewport {
        label: "Desktop",
        width: 1920,
        height: 1080,
        class: ViewportClass::Desktop,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportValidationCase {
    pub viewport: RequiredViewport,
    pub checks: &'static [ViewportValidationCheck],
    pub artifact: &'static str,
}

pub const REQUIRED_VIEWPORT_VALIDATION_CASES: [ViewportValidationCase; 5] = [
    ViewportValidationCase {
        viewport: REQUIRED_VIEWPORTS[0],
        checks: &REQUIRED_VIEWPORT_VALIDATION_CHECKS,
        artifact: UI_VALIDATION_ARTIFACT,
    },
    ViewportValidationCase {
        viewport: REQUIRED_VIEWPORTS[1],
        checks: &REQUIRED_VIEWPORT_VALIDATION_CHECKS,
        artifact: UI_VALIDATION_ARTIFACT,
    },
    ViewportValidationCase {
        viewport: REQUIRED_VIEWPORTS[2],
        checks: &REQUIRED_VIEWPORT_VALIDATION_CHECKS,
        artifact: UI_VALIDATION_ARTIFACT,
    },
    ViewportValidationCase {
        viewport: REQUIRED_VIEWPORTS[3],
        checks: &REQUIRED_VIEWPORT_VALIDATION_CHECKS,
        artifact: UI_VALIDATION_ARTIFACT,
    },
    ViewportValidationCase {
        viewport: REQUIRED_VIEWPORTS[4],
        checks: &REQUIRED_VIEWPORT_VALIDATION_CHECKS,
        artifact: UI_VALIDATION_ARTIFACT,
    },
];

#[must_use]
pub fn required_viewport(label: &str) -> Option<RequiredViewport> {
    REQUIRED_VIEWPORTS
        .iter()
        .copied()
        .find(|viewport| viewport.label == label)
}

#[must_use]
pub fn required_viewport_validation_case(label: &str) -> Option<ViewportValidationCase> {
    REQUIRED_VIEWPORT_VALIDATION_CASES
        .iter()
        .copied()
        .find(|case| case.viewport.label == label)
}
