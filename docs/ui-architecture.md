# UI Architecture

Sunbolt uses Dioxus Web for the browser UI. The UI should be production-oriented, responsive, and adaptive across mobile, tablet, laptop, and desktop devices.

## UI Ownership

`sunbolt-ui` owns:

- Web UI shell.
- Pages and reusable components.
- Browser terminal integration.
- Client-side session workspace state.
- Responsive and adaptive layout behavior.

Critical backend behavior must stay behind explicit Axum control-plane boundaries. Authentication, authorization, terminal lifecycle, node enrollment, audit logging, and database access should not be hidden in Dioxus server functions when that would make them harder to test.

## Main Areas

The product UI should cover:

- Login and MFA.
- Dashboard.
- Server and node management.
- Terminal workspace.
- Terminal sessions.
- Access history.
- Audit logs.
- Users, teams, and roles.
- Settings and security.

## Terminal Workspace

The terminal workspace should support multiple terminal tabs and clear lifecycle actions:

- Open new terminal.
- Switch active terminal.
- Close UI tab without killing the PTY by default.
- Detach terminal.
- Reattach existing terminal.
- Explicitly terminate terminal.
- Restore session list after page reload.

Close tab, detach, and terminate must remain distinct in the UI.

## Desktop and Laptop

Desktop and laptop layouts should favor dense control-plane workflows:

- Full navigation.
- Multi-tab terminal workspace.
- Tables with search, filters, and pagination.
- Terminal viewport using most available screen space.
- Audit and access history views optimized for scanning.

## Tablet

Tablet layouts should use two-pane patterns where width allows:

- Node list plus detail or terminal pane.
- Compact terminal controls.
- Portrait and landscape support.
- Tables that can collapse into compact rows.

## Mobile

Mobile should be terminal-first:

- Full-screen terminal workspace.
- Bottom navigation or compact top navigation.
- Session switcher through dropdown, segmented control, or bottom sheet.
- Node selector and session actions in bottom sheets.
- Login and MFA flows usable with the keyboard open.
- Accessory toolbar for keys and actions that mobile keyboards handle poorly.

The mobile terminal accessory toolbar should include:

- `Ctrl`
- `Esc`
- `Tab`
- Arrow keys
- Paste
- Resize or reconnect
- Detach
- Terminate session

## Required Viewports

Baseline validation viewports:

- iPhone 11 Pro: `375x812`.
- iPad 11 Pro portrait: `834x1194`.
- iPad 11 Pro landscape: `1194x834`.
- Laptop: `1366x768`.
- Desktop: `1920x1080`.

Validation should check navigation, terminal usability, session switching, login, MFA, and absence of overlapping text or controls.

## Component Direction

Reusable UI pieces should be extracted for:

- Layout.
- Buttons and icon buttons.
- Status badges.
- Table and list renderers.
- Forms.
- Modal dialogs.
- Mobile bottom sheets.
- API client helpers.
- Terminal workspace state.

Shared controls should be implemented once and reused instead of duplicating per page.
