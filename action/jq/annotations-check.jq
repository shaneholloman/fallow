def san: gsub("\n"; " ") | gsub("\r"; " ") | gsub("%"; "%25");
def nl: "%0A";
def pm: $ENV.PKG_MANAGER // "npm";
def remove_cmd(pkg): if pm == "pnpm" then "pnpm remove \(pkg)" elif pm == "yarn" then "yarn remove \(pkg)" else "npm uninstall \(pkg)" end;
def add_cmd(pkg): if pm == "pnpm" then "pnpm add \(pkg)" elif pm == "yarn" then "yarn add \(pkg)" else "npm install \(pkg)" end;
[
  (.unused_files[]? |
    "::warning file=\(.path | san),title=Unused file::This file is not imported by any other module and unreachable from entry points.\(nl)Consider removing it or importing it where needed."),
  (.unused_exports[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Unused export::\(if .is_re_export then "Re-exported" else "Exported" end) \(if .is_type_only then "type" else "value" end) '\(.export_name | san)' is never imported by other modules.\(nl)\(nl)If this export is part of a public API, consider adding it to the entry configuration.\(nl)Otherwise, remove the export keyword or delete the declaration."),
  (.unused_types[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Unused type::\(if .is_re_export then "Re-exported" else "Exported" end) type '\(.export_name | san)' is never imported by other modules.\(nl)\(nl)If only used internally, remove the export keyword."),
  (.unused_dependencies[]? |
    "::warning file=\(.path | san)\(if .line > 0 then ",line=\(.line)" else "" end),title=Unused dependency::Package '\(.package_name | san)' is listed in dependencies but never imported anywhere in the project.\(nl)\(nl)Run: \(remove_cmd(.package_name | san))"),
  (.unused_dev_dependencies[]? |
    "::warning file=\(.path | san)\(if .line > 0 then ",line=\(.line)" else "" end),title=Unused devDependency::Package '\(.package_name | san)' is listed in devDependencies but never imported.\(nl)\(nl)Run: \(remove_cmd(.package_name | san))"),
  (.unused_optional_dependencies[]? |
    "::warning file=\(.path | san)\(if .line > 0 then ",line=\(.line)" else "" end),title=Unused optionalDependency::Package '\(.package_name | san)' is listed in optionalDependencies but never imported.\(nl)\(nl)Run: \(remove_cmd(.package_name | san))"),
  (.unused_enum_members[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Unused enum member::Enum member '\(.parent_name | san).\(.member_name | san)' is never referenced in the codebase.\(nl)\(nl)Consider removing it to keep the enum minimal."),
  (.unused_class_members[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Unused class member::Class member '\(.parent_name | san).\(.member_name | san)' is never referenced.\(nl)\(nl)Consider removing it or marking it as private."),
  (.unresolved_imports[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Unresolved import::Import '\(.specifier | san)' could not be resolved to a file or package.\(nl)\(nl)Check for typos, missing dependencies, or incorrect path aliases."),
  (.unlisted_dependencies[]? | (.package_name | san) as $pkg | .imported_from[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Unlisted dependency::Package '\($pkg)' is imported here but not listed in package.json.\(nl)\(nl)Run: \(add_cmd($pkg))"),
  (.duplicate_exports[]? | (.export_name | san) as $name | .locations as $locs | .locations[]? |
    "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=Duplicate export::Export '\($name)' is defined in \($locs | length) modules:\(nl)\($locs | map("  \u2022 " + (.path | san) + ":" + (.line | tostring)) | join(nl))\(nl)\(nl)This causes ambiguity for consumers. Keep one canonical location."),
  (.circular_dependencies[]? |
    "::warning file=\(.files[0] | san)\(if .line > 0 then ",line=\(.line),col=\(.col + 1)" else "" end),title=Circular dependency::Circular import chain detected:\(nl)\(.files | map(san) | join(" \u2192 ")) \u2192 \(.files[0] | san)\(nl)\(nl)Circular dependencies can cause initialization bugs and make code harder to reason about.\(nl)Consider extracting shared logic into a separate module."),
  (.type_only_dependencies[]? |
    "::warning file=\(.path | san)\(if .line > 0 then ",line=\(.line)" else "" end),title=Type-only dependency::Package '\(.package_name | san)' is only used via type imports.\(nl)\(nl)Move it from dependencies to devDependencies to reduce production bundle size.")
] | .[]
