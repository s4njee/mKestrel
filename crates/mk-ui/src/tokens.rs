//! Rust mirror of the design tokens (E1-S1). The single styling source of
//! truth is `assets/main.css`; these constants exist for anything logic or
//! inline styles need (progress-bar colours, status tones, preview stripes).
//! The values here MUST match the CSS — the E14-S3 visual-diff gate is what
//! enforces that over time.

pub mod color {
    pub const BG: &str = "#171512";
    pub const BG_ALT: &str = "#1b1814";
    pub const BG_STRIP: &str = "#171512";
    pub const TEXT: &str = "#ede8e0";
    pub const ACCENT: &str = "#3b6fe0";
    pub const ACCENT_ON: &str = "#ffffff";
    pub const ACCENT_WASH: &str = "rgba(59,111,224,.10)";
    pub const WARN: &str = "#c9803a";
    pub const ERROR: &str = "#d08a6a";
    pub const HAIRLINE: &str = "#221e19";
    pub const BORDER_CTL: &str = "#353028";
    pub const PROGRESS_TRACK: &str = "#2a251f";
    pub const PREVIEW_STRIPE_1: &str = "#1f1c18";
    pub const PREVIEW_STRIPE_2: &str = "#1b1814";
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
