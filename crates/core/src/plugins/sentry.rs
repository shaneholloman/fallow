use super::Plugin;

pub struct SentryPlugin;

const ENABLERS: &[&str] = &[
    "@sentry/nextjs",
    "@sentry/react",
    "@sentry/node",
    "@sentry/browser",
];

const ALWAYS_USED: &[&str] = &[
    "sentry.client.config.{ts,js,mjs}",
    "sentry.server.config.{ts,js,mjs}",
    "sentry.edge.config.{ts,js,mjs}",
];

const TOOLING_DEPENDENCIES: &[&str] = &[
    "@sentry/nextjs",
    "@sentry/react",
    "@sentry/node",
    "@sentry/browser",
    "@sentry/cli",
    "@sentry/webpack-plugin",
    "@sentry/vite-plugin",
];

impl Plugin for SentryPlugin {
    fn name(&self) -> &'static str {
        "Sentry"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn always_used(&self) -> &'static [&'static str] {
        ALWAYS_USED
    }

    fn tooling_dependencies(&self) -> &'static [&'static str] {
        TOOLING_DEPENDENCIES
    }
}
