#[path = "integration_test/common.rs"]
mod common;

#[path = "integration_test/barrel_exports.rs"]
mod barrel_exports;
#[path = "integration_test/basic_analysis.rs"]
mod basic_analysis;
#[path = "integration_test/caching.rs"]
mod caching;
#[path = "integration_test/css_modules.rs"]
mod css_modules;
#[path = "integration_test/dependencies.rs"]
mod dependencies;
#[path = "integration_test/duplicates.rs"]
mod duplicates;
#[path = "integration_test/dynamic_imports.rs"]
mod dynamic_imports;
#[path = "integration_test/external_plugins.rs"]
mod external_plugins;
#[path = "integration_test/extraction.rs"]
mod extraction;
#[path = "integration_test/false_positive_fixes.rs"]
mod false_positive_fixes;
#[path = "integration_test/framework_convention_coverage_astro_gatsby.rs"]
mod framework_convention_coverage_astro_gatsby;
#[path = "integration_test/framework_convention_coverage_common.rs"]
mod framework_convention_coverage_common;
#[path = "integration_test/framework_convention_coverage_router.rs"]
mod framework_convention_coverage_router;
#[path = "integration_test/framework_convention_coverage_vitepress.rs"]
mod framework_convention_coverage_vitepress;
#[path = "integration_test/frameworks.rs"]
mod frameworks;
#[path = "integration_test/html_entry.rs"]
mod html_entry;
#[path = "integration_test/member_detection.rs"]
mod member_detection;
#[path = "integration_test/rules_config.rs"]
mod rules_config;
#[path = "integration_test/sfc_parsing.rs"]
mod sfc_parsing;
#[path = "integration_test/unreachable_exports.rs"]
mod unreachable_exports;
#[path = "integration_test/workspaces.rs"]
mod workspaces;

#[path = "integration_test/boundary_violations.rs"]
mod boundary_violations;
#[path = "integration_test/config_file_loading.rs"]
mod config_file_loading;
#[path = "integration_test/css_modules_unused.rs"]
mod css_modules_unused;
#[path = "integration_test/production_mode.rs"]
mod production_mode;
#[path = "integration_test/re_export_chains.rs"]
mod re_export_chains;
#[path = "integration_test/suppression_comments.rs"]
mod suppression_comments;
#[path = "integration_test/test_only_deps.rs"]
mod test_only_deps;
#[path = "integration_test/type_only_deps.rs"]
mod type_only_deps;
#[path = "integration_test/unused_enum_members.rs"]
mod unused_enum_members;
#[path = "integration_test/workspace_cross_imports.rs"]
mod workspace_cross_imports;
