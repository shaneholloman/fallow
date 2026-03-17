use super::Plugin;

pub struct ReactRouterPlugin;

impl Plugin for ReactRouterPlugin {
    fn name(&self) -> &'static str {
        "react_router"
    }
}
