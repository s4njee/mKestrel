//! Rust mirror of the design tokens (E1-S1). The single styling source of
//! truth is `assets/main.css`; these constants exist for anything logic or
//! inline styles need (progress-bar colours, status tones, preview stripes).
//! The values here MUST match the CSS — the E14-S3 visual-diff gate is what
//! enforces that over time.

pub mod color {
    pub const BG: &str = "#0b0c0c";
    pub const BG_ALT: &str = "#0e100f";
    pub const BG_STRIP: &str = "#0d0f0e";
    pub const TEXT: &str = "#eef1f0";
    pub const ACCENT: &str = "#00c48f";
    pub const ACCENT_ON: &str = "#0b0f0e";
    pub const ACCENT_WASH: &str = "rgba(0,196,143,.06)";
    pub const WARN: &str = "#e0a83c";
    pub const ERROR: &str = "#e07a6a";
    pub const HAIRLINE: &str = "rgba(255,255,255,.07)";
    pub const BORDER_CTL: &str = "rgba(255,255,255,.12)";
    pub const PROGRESS_TRACK: &str = "rgba(255,255,255,.08)";
    pub const PREVIEW_STRIPE_1: &str = "#141615";
    pub const PREVIEW_STRIPE_2: &str = "#101211";
}

/// The text-alpha steps, used where a value needs a specific dimming level.
pub const TEXT_ALPHAS: [&str; 7] = ["70", "60", "45", "40", "35", "30", "25"];

/// Named typographic classes (must exist in `assets/main.css`).
pub mod type_class {
    pub const SCREEN_TITLE: &str = "t-screen-title";
    pub const PANE_TITLE: &str = "t-pane-title";
    pub const TABLE_NAME: &str = "t-table-name";
    pub const LIST_ROW_TITLE: &str = "t-list-row-title";
    pub const DATA_CELL: &str = "t-data-cell";
    pub const COL_HEADER: &str = "t-col-header";
    pub const SECTION_LABEL: &str = "t-section-label";
    pub const MODE: &str = "t-mode";
    pub const METRIC: &str = "t-metric";
    pub const BIG_READOUT: &str = "t-big-readout";
}
