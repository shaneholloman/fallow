import type { FallowCheckResult } from "./types.js";

export const countCheckIssues = (result: FallowCheckResult | null): number => {
  if (!result) {
    return 0;
  }

  return (
    result.unused_files.length +
    result.unused_exports.length +
    result.unused_types.length +
    result.unused_dependencies.length +
    result.unused_dev_dependencies.length +
    (result.unused_optional_dependencies?.length ?? 0) +
    result.unused_enum_members.length +
    result.unused_class_members.length +
    result.unresolved_imports.length +
    result.unlisted_dependencies.length +
    result.duplicate_exports.length +
    (result.type_only_dependencies?.length ?? 0) +
    (result.circular_dependencies?.length ?? 0)
  );
};
