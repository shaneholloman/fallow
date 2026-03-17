use super::Plugin;

pub struct RemixPlugin;

impl Plugin for RemixPlugin {
    fn name(&self) -> &'static str {
        "remix"
    }
}
