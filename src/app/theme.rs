use eframe::egui::{self, Color32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTheme {
    OneDarkProDarker,
    GitHubDark,
    MonokaiPro,
    TokyoNight,
    LightModern,
}

impl ColorTheme {
    pub fn all() -> &'static [ColorTheme] {
        &[
            ColorTheme::OneDarkProDarker,
            ColorTheme::GitHubDark,
            ColorTheme::MonokaiPro,
            ColorTheme::TokyoNight,
            ColorTheme::LightModern,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ColorTheme::OneDarkProDarker => "One Dark Pro Darker",
            ColorTheme::GitHubDark => "GitHub Dark",
            ColorTheme::MonokaiPro => "Monokai Pro",
            ColorTheme::TokyoNight => "Tokyo Night",
            ColorTheme::LightModern => "VS Code Light Modern",
        }
    }

    pub fn is_dark(&self) -> bool {
        !matches!(self, ColorTheme::LightModern)
    }

    pub fn bg_color(&self) -> Color32 {
        match self {
            ColorTheme::OneDarkProDarker => Color32::from_rgb(30, 34, 39), // #1E2227
            ColorTheme::GitHubDark => Color32::from_rgb(13, 17, 23),       // #0D1117
            ColorTheme::MonokaiPro => Color32::from_rgb(45, 42, 46),       // #2D2A2E
            ColorTheme::TokyoNight => Color32::from_rgb(26, 27, 38),       // #1A1B26
            ColorTheme::LightModern => Color32::from_rgb(255, 255, 255),   // #FFFFFF
        }
    }

    pub fn secondary_bg(&self) -> Color32 {
        match self {
            ColorTheme::OneDarkProDarker => Color32::from_rgb(24, 26, 31),
            ColorTheme::GitHubDark => Color32::from_rgb(22, 27, 34),
            ColorTheme::MonokaiPro => Color32::from_rgb(34, 31, 34),
            ColorTheme::TokyoNight => Color32::from_rgb(22, 22, 30),
            ColorTheme::LightModern => Color32::from_rgb(243, 243, 243),
        }
    }

    pub fn border_color(&self) -> Color32 {
        match self {
            ColorTheme::OneDarkProDarker => Color32::from_rgb(40, 44, 52), // #282C34 subtle border
            ColorTheme::GitHubDark => Color32::from_rgb(48, 54, 61),
            ColorTheme::MonokaiPro => Color32::from_rgb(64, 60, 65),
            ColorTheme::TokyoNight => Color32::from_rgb(41, 46, 66),
            ColorTheme::LightModern => Color32::from_rgb(229, 229, 229),
        }
    }

    pub fn accent_color(&self) -> Color32 {
        match self {
            ColorTheme::OneDarkProDarker => Color32::from_rgb(97, 175, 239), // #61AFEF Sky Blue
            ColorTheme::GitHubDark => Color32::from_rgb(31, 111, 235),
            ColorTheme::MonokaiPro => Color32::from_rgb(255, 97, 136),
            ColorTheme::TokyoNight => Color32::from_rgb(122, 162, 247),
            ColorTheme::LightModern => Color32::from_rgb(0, 120, 215),
        }
    }

    pub fn text_color(&self) -> Color32 {
        match self {
            ColorTheme::OneDarkProDarker => Color32::from_rgb(220, 225, 232), // #DCDFE4 High clarity text
            ColorTheme::GitHubDark => Color32::from_rgb(201, 209, 217),
            ColorTheme::MonokaiPro => Color32::from_rgb(252, 252, 250),
            ColorTheme::TokyoNight => Color32::from_rgb(169, 177, 214),
            ColorTheme::LightModern => Color32::from_rgb(51, 51, 51),
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = if self.is_dark() {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        let bg = self.bg_color();
        let border = self.border_color();
        let text = self.text_color();
        let accent = self.accent_color();

        visuals.override_text_color = Some(text);
        visuals.panel_fill = bg;
        visuals.window_fill = bg;
        visuals.extreme_bg_color = bg;
        visuals.faint_bg_color = self.secondary_bg();
        visuals.window_stroke = egui::Stroke::new(1.0_f32, border);
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border);
        visuals.widgets.inactive.bg_fill = bg;
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
        visuals.widgets.hovered.bg_fill = if self.is_dark() {
            Color32::from_rgb(44, 49, 58)
        } else {
            Color32::from_rgb(230, 230, 230)
        };
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        visuals.widgets.active.bg_fill = accent;
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
        visuals.selection.bg_fill = if self.is_dark() {
            Color32::from_rgb(61, 69, 86)
        } else {
            Color32::from_rgba_unmultiplied(180, 205, 240, 220)
        };
        visuals.selection.stroke = egui::Stroke::NONE;

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.5));
        style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.menu_margin = egui::Margin::symmetric(6, 6);
        ctx.set_style(style);
    }
}
