use ratatui::Frame;

pub mod overlay {
    pub fn test_centered_positioning() {}
    pub fn test_top_anchor() {}
    pub fn test_small_terminal() {}
    pub fn test_no_ghost_artifacts() {}
}

pub mod input {
    pub fn test_cursor_rendering() {}
    pub fn test_focus_state() {}
    pub fn test_text_entry() {}
}

pub mod loading {
    pub fn test_appears_during_async() {}
    pub fn test_clears_after_completion() {}
    pub fn test_animation_frames() {}
}

pub fn capture_frame(frame: &Frame<'_>) -> String {
    format!("Frame area: {:?}", frame.area())
}
