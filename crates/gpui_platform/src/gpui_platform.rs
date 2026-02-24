pub use gpui::{Application, BackgroundExecutor};

pub fn application() -> Application {
    Application::new()
}

pub fn headless() -> Application {
    Application::headless()
}

pub fn background_executor() -> BackgroundExecutor {
    gpui::background_executor()
}
