use super::Plugin;

pub struct MswPlugin;

const ENABLERS: &[&str] = &["msw"];

const ENTRY_PATTERNS: &[&str] = &[
    "mocks/**/*.{ts,tsx,js,jsx}",
    "src/mocks/**/*.{ts,tsx,js,jsx}",
    // Feature-scoped mocks (common in modular codebases)
    "**/mocks/**/*.{ts,tsx,js,jsx}",
];

const ALWAYS_USED: &[&str] = &["public/mockServiceWorker.js"];

const TOOLING_DEPENDENCIES: &[&str] = &["msw", "msw-storybook-addon"];

impl Plugin for MswPlugin {
    fn name(&self) -> &'static str {
        "Mock Service Worker"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn entry_patterns(&self) -> &'static [&'static str] {
        ENTRY_PATTERNS
    }

    fn always_used(&self) -> &'static [&'static str] {
        ALWAYS_USED
    }

    fn tooling_dependencies(&self) -> &'static [&'static str] {
        TOOLING_DEPENDENCIES
    }
}
