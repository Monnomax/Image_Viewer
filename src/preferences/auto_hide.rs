#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityMode {
    AutoHide,
    AlwaysShow,
    NeverShow,
}

impl VisibilityMode {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "auto-hide" => Self::AutoHide,
            "never-show" => Self::NeverShow,
            _ => Self::AlwaysShow,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::AutoHide => "auto-hide",
            Self::AlwaysShow => "always-show",
            Self::NeverShow => "never-show",
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            0 => Self::AutoHide,
            2 => Self::NeverShow,
            _ => Self::AlwaysShow,
        }
    }

    pub fn as_index(self) -> u32 {
        match self {
            Self::AutoHide => 0,
            Self::AlwaysShow => 1,
            Self::NeverShow => 2,
        }
    }

    pub fn from_cursor_index(index: u32) -> Self {
        match index {
            0 => Self::AutoHide,
            _ => Self::AlwaysShow,
        }
    }

    pub fn as_cursor_index(self) -> u32 {
        match self {
            Self::AutoHide => 0,
            Self::AlwaysShow => 1,
            Self::NeverShow => 1,
        }
    }

    pub fn is_visible(self) -> bool {
        !matches!(self, Self::NeverShow)
    }
}

#[cfg(test)]
mod tests {
    use super::VisibilityMode;

    #[test]
    fn maps_visibility_modes_to_settings_values() {
        assert_eq!(VisibilityMode::AutoHide.as_setting(), "auto-hide");
        assert_eq!(VisibilityMode::AlwaysShow.as_setting(), "always-show");
        assert_eq!(VisibilityMode::NeverShow.as_setting(), "never-show");
    }

    #[test]
    fn parses_index_values_for_visibility_rows() {
        assert_eq!(VisibilityMode::from_index(0), VisibilityMode::AutoHide);
        assert_eq!(VisibilityMode::from_index(1), VisibilityMode::AlwaysShow);
        assert_eq!(VisibilityMode::from_index(2), VisibilityMode::NeverShow);
    }
}
