use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Comment, Program};
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::ExportInfo;
use crate::ModuleInfo;
use crate::astro::{is_astro_file, parse_astro_to_module};
use crate::css::{is_css_file, parse_css_to_module};
use crate::html::{is_html_file, parse_html_to_module};
use crate::mdx::{is_mdx_file, parse_mdx_to_module};
use crate::sfc::{is_sfc_file, parse_sfc_to_module};
use crate::visitor::ModuleInfoExtractor;
use fallow_types::discover::FileId;
use fallow_types::extract::ImportInfo;

/// Parse source text into a [`ModuleInfo`].
///
/// When `need_complexity` is false the per-function complexity visitor is
/// skipped, saving one full AST walk per file.  The dead-code analysis
/// pipeline never consumes complexity data, so callers that only need
/// imports/exports should pass `false`.
pub fn parse_source_to_module(
    file_id: FileId,
    path: &Path,
    source: &str,
    content_hash: u64,
    need_complexity: bool,
) -> ModuleInfo {
    if is_sfc_file(path) {
        return parse_sfc_to_module(file_id, path, source, content_hash);
    }
    if is_astro_file(path) {
        return parse_astro_to_module(file_id, source, content_hash);
    }
    if is_mdx_file(path) {
        return parse_mdx_to_module(file_id, source, content_hash);
    }
    if is_css_file(path) {
        return parse_css_to_module(file_id, path, source, content_hash);
    }
    if is_html_file(path) {
        return parse_html_to_module(file_id, source, content_hash);
    }

    let source_type = SourceType::from_path(path).unwrap_or_default();
    let allocator = Allocator::default();
    let parser_return = Parser::new(&allocator, source, source_type).parse();

    // Parse suppression comments from AST comments initially;
    // re-parsed from retry comments below if JSX retry succeeds.
    let mut suppressions =
        crate::suppress::parse_suppressions(&parser_return.program.comments, source);

    // Extract imports/exports even if there are parse errors
    let mut extractor = ModuleInfoExtractor::new();
    extractor.visit_program(&parser_return.program);

    // Detect unused import bindings via oxc_semantic scope analysis
    let mut unused_bindings =
        compute_unused_import_bindings(&parser_return.program, &extractor.imports);

    // Line offsets are always needed (error location reporting in analysis).
    let line_offsets = fallow_types::extract::compute_line_offsets(source);

    // Per-function complexity metrics: only computed when the caller needs them
    // (e.g. the `health` command).  The dead-code pipeline never reads this.
    let mut complexity = if need_complexity {
        crate::complexity::compute_complexity(&parser_return.program, line_offsets.clone())
    } else {
        Vec::new()
    };

    // If parsing produced very few results relative to source size (likely parse errors
    // from Flow types or JSX in .js files), retry with JSX/TSX source type as a fallback.
    let total_extracted =
        extractor.exports.len() + extractor.imports.len() + extractor.re_exports.len();
    let mut used_retry = false;
    if total_extracted == 0 && source.len() > 100 && !source_type.is_jsx() {
        let jsx_type = if source_type.is_typescript() {
            SourceType::tsx()
        } else {
            SourceType::jsx()
        };
        let allocator2 = Allocator::default();
        let retry_return = Parser::new(&allocator2, source, jsx_type).parse();
        let mut retry_extractor = ModuleInfoExtractor::new();
        retry_extractor.visit_program(&retry_return.program);
        let retry_total = retry_extractor.exports.len()
            + retry_extractor.imports.len()
            + retry_extractor.re_exports.len();
        if retry_total > total_extracted {
            unused_bindings =
                compute_unused_import_bindings(&retry_return.program, &retry_extractor.imports);
            // Recompute complexity from the successful retry parse (only if requested)
            if need_complexity {
                complexity = crate::complexity::compute_complexity(
                    &retry_return.program,
                    line_offsets.clone(),
                );
            }
            // Re-parse suppressions from the retry's comments (not the original failed parse)
            suppressions =
                crate::suppress::parse_suppressions(&retry_return.program.comments, source);
            // Apply @public tags from the retry parse's comments (not the original failed parse)
            apply_jsdoc_public_tags(
                &mut retry_extractor.exports,
                &retry_return.program.comments,
                source,
            );
            extractor = retry_extractor;
            used_retry = true;
        }
    }

    // Apply JSDoc @public tags from the original parse (skip if retry was used above)
    if !used_retry {
        apply_jsdoc_public_tags(
            &mut extractor.exports,
            &parser_return.program.comments,
            source,
        );
    }

    let mut info = extractor.into_module_info(file_id, content_hash, suppressions);
    info.unused_import_bindings = unused_bindings;
    info.line_offsets = line_offsets;
    info.complexity = complexity;

    info
}

/// Apply JSDoc `@public` tags to exports by matching leading JSDoc comments.
///
/// `Comment.attached_to` points to the `export` keyword byte offset, while
/// `ExportInfo.span` stores the identifier byte offset (e.g., `foo` in
/// `export const foo`). This function bridges the gap: it collects `@public`
/// comment attachment offsets, then for each export finds the nearest preceding
/// attachment point and validates it's part of the same export statement.
fn apply_jsdoc_public_tags(exports: &mut [ExportInfo], comments: &[Comment], source: &str) {
    if exports.is_empty() || comments.is_empty() {
        return;
    }

    // Collect byte offsets where @public JSDoc comments attach
    let mut public_offsets: Vec<u32> = Vec::new();
    for comment in comments {
        if comment.is_jsdoc() {
            let content_span = comment.content_span();
            let start = content_span.start as usize;
            let end = (content_span.end as usize).min(source.len());
            if start < end && has_public_tag(&source[start..end]) {
                public_offsets.push(comment.attached_to);
            }
        }
    }

    if public_offsets.is_empty() {
        return;
    }

    public_offsets.sort_unstable();

    for export in exports.iter_mut() {
        // Skip synthetic exports (re-export entries with span 0..0)
        if export.span.start == 0 && export.span.end == 0 {
            continue;
        }

        // Check for exact match first (e.g., `export default` where span = decl span)
        if public_offsets.binary_search(&export.span.start).is_ok() {
            export.is_public = true;
            continue;
        }

        // Find the largest @public offset that is <= this export's span start
        let idx = public_offsets.partition_point(|&o| o <= export.span.start);
        if idx > 0 {
            let offset = public_offsets[idx - 1] as usize;
            let export_start = export.span.start as usize;
            if offset < export_start && export_start <= source.len() {
                let between = &source[offset..export_start];
                // Validate: the text between the comment attachment and the identifier
                // should be a clean export preamble (e.g., "export const ") with no
                // statement boundaries separating them.
                if between.starts_with("export") && !between.contains(';') && !between.contains('}')
                {
                    export.is_public = true;
                }
            }
        }
    }
}

/// Check if a byte is an identifier-continuation character (alphanumeric or `_`).
const fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Check if a JSDoc comment body contains a `@public` or `@api public` tag.
fn has_public_tag(comment_text: &str) -> bool {
    // Check for @public (standalone tag, not part of another word)
    for (i, _) in comment_text.match_indices("@public") {
        let after = i + "@public".len();
        // Must not be followed by an identifier char (alphanumeric or _)
        if after >= comment_text.len() || !is_ident_char(comment_text.as_bytes()[after]) {
            return true;
        }
    }
    // Check for @api public (TSDoc convention)
    for (i, _) in comment_text.match_indices("@api") {
        let after = i + "@api".len();
        // @api must be a standalone tag (not @apipublic, @api_foo)
        if after < comment_text.len() && !is_ident_char(comment_text.as_bytes()[after]) {
            let rest = comment_text[after..].trim_start();
            if rest.starts_with("public") {
                let after_public = "public".len();
                if after_public >= rest.len() || !is_ident_char(rest.as_bytes()[after_public]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Use `oxc_semantic` to find import bindings that are never referenced in the file.
///
/// An import like `import { foo } from './utils'` where `foo` is never used
/// anywhere in the file should not count as a reference to the `foo` export.
/// This improves unused-export detection precision.
///
/// Note: `get_resolved_references` counts both value-context and type-context
/// references. A value import used only as a type annotation (`const x: Foo`)
/// will have a type-position reference and will NOT appear in the unused list.
/// This is correct: `import { Foo }` (without `type`) may be needed at runtime.
pub fn compute_unused_import_bindings(
    program: &Program<'_>,
    imports: &[ImportInfo],
) -> Vec<String> {
    use oxc_semantic::SemanticBuilder;

    // Skip files with no imports
    if imports.is_empty() {
        return Vec::new();
    }

    let semantic_ret = SemanticBuilder::new().build(program);
    let semantic = semantic_ret.semantic;
    let scoping = semantic.scoping();
    let root_scope = scoping.root_scope_id();

    let mut unused = Vec::new();
    for import in imports {
        // Side-effect imports have no binding
        if import.local_name.is_empty() {
            continue;
        }
        // Look up the import binding in the module scope
        let name = oxc_span::Ident::from(import.local_name.as_str());
        if let Some(symbol_id) = scoping.get_binding(root_scope, name)
            && scoping.get_resolved_references(symbol_id).count() == 0
        {
            unused.push(import.local_name.clone());
        }
    }
    unused
}
