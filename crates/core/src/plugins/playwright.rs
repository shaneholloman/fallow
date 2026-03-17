use super::Plugin;

pub struct PlaywrightPlugin;

impl Plugin for PlaywrightPlugin {
    fn name(&self) -> &'static str {
        "playwright"
    }
}
