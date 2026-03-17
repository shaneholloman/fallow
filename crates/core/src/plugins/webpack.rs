use super::Plugin;

pub struct WebpackPlugin;

impl Plugin for WebpackPlugin {
    fn name(&self) -> &'static str {
        "webpack"
    }
}
