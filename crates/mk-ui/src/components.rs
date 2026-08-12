//! Primitive components (E1-S2). Every primitive is styled purely through
//! `assets/main.css` classes — no raw colour/measure literals here.

use dioxus::prelude::*;

/// Status-tone modifier shared by dots and other small indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Warn,
    Error,
    Muted,
}

impl Tone {
    pub fn class(self) -> &'static str {
        match self {
            Tone::Accent => "accent",
            Tone::Warn => "warn",
            Tone::Error => "error",
            Tone::Muted => "muted",
        }
    }
}

/// Chip variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Plain,
    Accent,
    OutlineAccent,
    Warn,
    Error,
}

impl ChipKind {
    pub fn class(self) -> &'static str {
        match self {
            ChipKind::Plain => "",
            ChipKind::Accent => "accent",
            ChipKind::OutlineAccent => "outline-accent",
            ChipKind::Warn => "warn",
            ChipKind::Error => "error",
        }
    }
}

/// Horizontal or vertical hairline divider.
#[component]
pub fn Hairline(vertical: Option<bool>) -> Element {
    let class = if vertical.unwrap_or(false) {
        "hairline-v"
    } else {
        "hairline"
    };
    rsx! { div { class: "{class}" } }
}

/// A 22px-tall table row; `selected` gets the accent wash + 2px accent border.
#[component]
pub fn Row(selected: Option<bool>, children: Element) -> Element {
    let class = if selected.unwrap_or(false) {
        "row selected"
    } else {
        "row"
    };
    rsx! { div { class: "{class}", {children} } }
}

/// Styled column header text (9.5px mono, .10em, uppercase, text-30).
#[component]
pub fn ColumnHeader(text: String) -> Element {
    rsx! { span { class: "t-col-header", "{text}" } }
}

/// A mono data cell; `right` right-aligns, `accent` colours the value.
#[component]
pub fn DataCell(
    text: String,
    right: Option<bool>,
    accent: Option<bool>,
    dim: Option<bool>,
) -> Element {
    let mut classes = vec!["data-cell".to_string()];
    if right.unwrap_or(false) {
        classes.push("right".into());
    }
    if accent.unwrap_or(false) {
        classes.push("accent".into());
    }
    if dim.unwrap_or(false) {
        classes.push("dim".into());
    }
    let class = classes.join(" ");
    rsx! { span { class: "{class}", "{text}" } }
}

/// Mono text chip.
#[component]
pub fn MonoChip(label: String, kind: Option<ChipKind>, disabled: Option<bool>) -> Element {
    let kind = kind.unwrap_or(ChipKind::Plain);
    let mut class = "chip".to_string();
    if !kind.class().is_empty() {
        class.push(' ');
        class.push_str(kind.class());
    }
    let disabled = disabled.unwrap_or(false);
    rsx! {
        button {
            class: "{class}",
            disabled: disabled,
            "{label}"
        }
    }
}

/// Outlined button.
#[component]
pub fn OutlineButton(
    label: String,
    onpress: Option<EventHandler<()>>,
    disabled: Option<bool>,
) -> Element {
    rsx! {
        button {
            class: "btn",
            disabled: disabled.unwrap_or(false),
            onclick: move |_| {
                if let Some(h) = onpress {
                    h.call(());
                }
            },
            "{label}"
        }
    }
}

/// Accent-filled primary button.
#[component]
pub fn AccentButton(
    label: String,
    onpress: Option<EventHandler<()>>,
    disabled: Option<bool>,
) -> Element {
    rsx! {
        button {
            class: "btn btn-accent",
            disabled: disabled.unwrap_or(false),
            onclick: move |_| {
                if let Some(h) = onpress {
                    h.call(());
                }
            },
            "{label}"
        }
    }
}

/// Destructive button (error text + error border).
#[component]
pub fn DangerButton(
    label: String,
    onpress: Option<EventHandler<()>>,
    disabled: Option<bool>,
) -> Element {
    rsx! {
        button {
            class: "btn btn-danger",
            disabled: disabled.unwrap_or(false),
            onclick: move |_| {
                if let Some(h) = onpress {
                    h.call(());
                }
            },
            "{label}"
        }
    }
}

/// 32×17 pill switch; knob 13px, accent fill when on.
#[component]
pub fn Switch(on: bool, onchange: Option<EventHandler<bool>>) -> Element {
    let class = if on { "switch on" } else { "switch" };
    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                if let Some(h) = onchange {
                    h.call(!on);
                }
            },
            div { class: "switch knob" }
        }
    }
}

/// 3px two-flex progress bar (accent fill / track), 1px gap.
#[component]
pub fn ProgressBar(percent: f64, compact: Option<bool>) -> Element {
    let pct = percent.clamp(0.0, 100.0);
    let class = if compact.unwrap_or(false) {
        "progress compact"
    } else {
        "progress"
    };
    let fill = format!("flex: {pct:.1}");
    let track = format!("flex: {:.1}", 100.0 - pct);
    rsx! {
        div {
            class: "{class}",
            div { class: "progress fill", style: "{fill}" }
            div { class: "progress track", style: "{track}" }
        }
    }
}

/// 7px status dot with a tone.
#[component]
pub fn StatusDot(tone: Option<Tone>) -> Element {
    let tone = tone.unwrap_or(Tone::Accent);
    let class = if tone == Tone::Accent {
        "status-dot".to_string()
    } else {
        format!("status-dot {}", tone.class())
    };
    rsx! { div { class: "{class}" } }
}

/// Underline input field (E8-S2). The caret `|` renders when focused.
#[component]
pub fn UnderlineField(label: String, value: String, focused: Option<bool>) -> Element {
    let focused = focused.unwrap_or(false);
    let class = if focused { "field focused" } else { "field" };
    let caret = if focused { "|" } else { "" };
    rsx! {
        label {
            class: "{class}",
            span { class: "field-label", "{label}" }
            span { class: "field-value", "{value}{caret}" }
        }
    }
}

/// N-bar histogram. Older bars are dimmer, the last five `.mid`, the newest
/// solid accent. Data values are normalized to the max.
#[component]
pub fn Histogram(data: Vec<f64>, height: Option<u32>) -> Element {
    let max = data.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let n = data.len();
    let h = height.unwrap_or(56);
    // Precompute bar classes/styles so the rsx body stays declarative
    // (no `let` bindings inside the `for` body).
    let bars: Vec<(String, String)> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let class = if i == n - 1 {
                "bar latest"
            } else if i + 5 >= n {
                "bar mid"
            } else {
                "bar"
            };
            let style = format!("height: {:.0}%", v / max * 100.0);
            (class.to_string(), style)
        })
        .collect();
    rsx! {
        div {
            class: "histogram",
            style: "height: {h}px",
            for (class, style) in bars {
                div { class: "{class}", style: "{style}" }
            }
        }
    }
}

/// The striped file-preview placeholder from E6-S1, with a mono caption.
#[component]
pub fn PreviewPlaceholder(caption: String, height: Option<u32>) -> Element {
    let h = height.unwrap_or(166);
    rsx! {
        div {
            class: "preview-striped",
            style: "height: {h}px; display:flex; align-items:center; justify-content:center",
            div {
                class: "t-data-cell",
                style: "color: var(--text-40)",
                "{caption}"
            }
        }
    }
}
