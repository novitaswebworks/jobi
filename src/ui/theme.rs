//! Colour themes. "mono" (also used when NO_COLOR is set) relies on modifiers only.
use crate::config::Config;
use ratatui::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Dark,
    Light,
    Mono,
}

#[derive(Clone, Copy, Debug)]
pub struct Palette(Kind);

impl Palette {
    pub fn new(c: &Config) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Self(match c.theme.as_str() {
            _ if no_color => Kind::Mono,
            "mono" => Kind::Mono,
            "light" => Kind::Light,
            _ => Kind::Dark,
        })
    }
    fn pick(self, dark: Color, light: Color) -> Style {
        match self.0 {
            Kind::Dark => Style::new().fg(dark),
            Kind::Light => Style::new().fg(light),
            Kind::Mono => Style::new(),
        }
    }
    pub fn accent(self) -> Style {
        self.pick(Color::Cyan, Color::Blue)
    }
    pub fn dim(self) -> Style {
        match self.0 {
            Kind::Mono => Style::new().add_modifier(Modifier::DIM),
            _ => self.pick(Color::DarkGray, Color::Gray),
        }
    }
    pub fn warn(self) -> Style {
        match self.0 {
            Kind::Mono => Style::new().bold(),
            _ => self.pick(Color::Yellow, Color::Rgb(176, 96, 0)),
        }
    }
    pub fn danger(self) -> Style {
        match self.0 {
            Kind::Mono => Style::new().bold().underlined(),
            _ => self.pick(Color::Red, Color::Red).bold(),
        }
    }
    pub fn selected(self) -> Style {
        match self.0 {
            Kind::Dark => Style::new().bg(Color::DarkGray).fg(Color::White),
            Kind::Light => Style::new().bg(Color::Indexed(153)).fg(Color::Black),
            Kind::Mono => Style::new().add_modifier(Modifier::REVERSED),
        }
    }
    pub fn tab_active(self) -> Style {
        match self.0 {
            Kind::Dark => Style::new().fg(Color::Black).bg(Color::Cyan).bold(),
            Kind::Light => Style::new().fg(Color::White).bg(Color::Blue).bold(),
            Kind::Mono => Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
        }
    }
    pub fn tab_idle(self) -> Style {
        self.pick(Color::Gray, Color::DarkGray)
    }
    pub fn stage(self, stage: &str) -> Style {
        match stage {
            "Wishlist" | "Withdrawn" => self.dim(),
            "Applied" => self.pick(Color::LightBlue, Color::Blue),
            "Screening" => self.pick(Color::Cyan, Color::Rgb(0, 120, 140)),
            "Interview" => self.warn(),
            "Offer" => self.pick(Color::LightGreen, Color::Rgb(0, 120, 0)),
            "Accepted" => self.pick(Color::Green, Color::Rgb(0, 120, 0)).bold(),
            "Rejected" => self.danger(),
            _ => Style::new(),
        }
    }
}
