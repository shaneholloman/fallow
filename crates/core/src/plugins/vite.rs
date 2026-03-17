use super::Plugin;

pub struct VitePlugin;

impl Plugin for VitePlugin {
    fn name(&self) -> &'static str {
        "vite"
    }
}
