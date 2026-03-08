/// Centralized color palette for FALCON-ASM UI.
/// Theme: "Neutral + Violet" — pure neutral dark base, violet used sparingly
/// only for interactive highlights (active tabs, hover, selection).
///
/// Design rule: almost everything is neutral gray/steel/amber/green.
/// Violet appears ONLY where the user needs to notice something is active
/// or hovered — making it pop without saturating the whole UI.
///
/// All colors use RGB for consistent rendering across terminal themes.
use ratatui::prelude::Color;

// ── The one accent color ──────────────────────────────────────────────────────
/// Electric violet — used ONLY for active tabs, hover bg, border hover, selection.
/// Everything else is neutral so this truly stands out.
const VIOLET:     Color = Color::Rgb(145, 95,  250);

// ── Background layers (pure neutral dark gray, no tint) ───────────────────────
/// Main app background — neutral dark gray, comfortable VS Code-like dark mode
pub const BG:         Color = Color::Rgb(28,  28,  32);
/// Elevated panels — slightly lighter
pub const BG_PANEL:   Color = Color::Rgb(36,  36,  40);
/// Raised surface (popups)
pub const BG_RAISED:  Color = Color::Rgb(44,  44,  50);
/// Hover / selection row — just barely tinted toward violet
pub const BG_HOVER:   Color = Color::Rgb(50,  46,  66);
/// Subtle separator
pub const BG_SEP:     Color = Color::Rgb(46,  46,  52);

// ── Interactive controls ──────────────────────────────────────────────────────
/// Idle/inactive — neutral muted gray
pub const IDLE:       Color = Color::Rgb(105, 105, 115);
/// Active/selected foreground — neutral near-white
pub const ACTIVE:     Color = Color::Rgb(210, 210, 218);
/// Hover background — violet (the one accent)
pub const HOVER_BG:   Color = VIOLET;
/// Hover foreground — white on violet
pub const HOVER_FG:   Color = Color::Rgb(255, 255, 255);

// ── Semantic states ───────────────────────────────────────────────────────────
/// Running / success — clean green
pub const RUNNING:    Color = Color::Rgb(88,  200, 120);
/// Paused / warning — warm amber
pub const PAUSED:     Color = Color::Rgb(220, 170, 55);
/// Danger / error / reset — clean red
pub const DANGER:     Color = Color::Rgb(210, 72,  68);

// ── General UI ───────────────────────────────────────────────────────────────
/// Accent = violet, for active tabs / titles / interactive highlights
pub const ACCENT:     Color = VIOLET;
/// Normal borders — barely-visible neutral dark
pub const BORDER:     Color = Color::Rgb(58,  58,  66);
/// Hovered border — violet
pub const BORDER_HOV: Color = VIOLET;
/// Hint / auxiliary text — neutral muted
pub const LABEL:      Color = Color::Rgb(105, 105, 115);
/// Normal body text — neutral off-white (cool, not warm-tinted)
pub const TEXT:       Color = Color::Rgb(210, 210, 218);

// ── Metrics (status bar + cache stats) ───────────────────────────────────────
/// Cycles — steel blue (neutral-cool, data color)
pub const METRIC_CYC: Color = Color::Rgb(110, 175, 220);
/// CPI — soft lavender (violet family, hints at the accent)
pub const METRIC_CPI: Color = Color::Rgb(175, 135, 245);
/// IPC — warm gold
pub const METRIC_IPC: Color = Color::Rgb(225, 180, 80);

// ── Cache per-level colors ────────────────────────────────────────────────────
/// I-cache — steel blue
pub const CACHE_I:    Color = Color::Rgb(110, 175, 220);
/// D-cache — soft green
pub const CACHE_D:    Color = Color::Rgb(88,  200, 148);
/// L2+ cache — amber
pub const CACHE_L2:   Color = Color::Rgb(220, 170, 55);

// ── Cache data visualization ──────────────────────────────────────────────────
/// Dirty cache line — violet-pink (accent family, visible but not jarring)
pub const DIRTY:      Color = Color::Rgb(195, 105, 250);
/// Dirty address dim — muted violet
pub const DIRTY_DIM:  Color = Color::Rgb(108, 62,  148);
/// CPI config panel — teal (distinct from everything else)
pub const CPI_PANEL:  Color = Color::Rgb(72,  195, 168);

// ── Editor / syntax highlighting ──────────────────────────────────────────────
/// Code labels — warm amber (distinct from neutral text)
pub const LABEL_Y:    Color = Color::Rgb(218, 178, 75);
/// Block comments — muted sage (clearly softer than normal text)
pub const COMMENT:    Color = Color::Rgb(98,  138, 112);
/// Immediate values — steel blue (same family as metrics)
pub const IMM_COLOR:  Color = Color::Rgb(115, 178, 235);
