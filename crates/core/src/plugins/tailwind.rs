use super::Plugin;

pub struct TailwindPlugin;

impl Plugin for TailwindPlugin {
    fn name(&self) -> &'static str {
        "tailwind"
    }
}
