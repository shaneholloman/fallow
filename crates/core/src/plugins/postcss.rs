use super::Plugin;

pub struct PostCssPlugin;

impl Plugin for PostCssPlugin {
    fn name(&self) -> &'static str {
        "postcss"
    }
}
