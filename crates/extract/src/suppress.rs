//! Inline suppression comment parsing.
//!
//! Parses `fallow-ignore-file` and `fallow-ignore-next-line` comments from
//! source files, supporting both `//` and `/* */` styles.

use oxc_ast::ast::Comment;

// Re-export types from fallow-types
pub use fallow_types::suppress::{IssueKind, Suppression};

/// Convert a byte offset to a 1-based line number.
fn byte_offset_to_line(source: &str, byte_offset: u32) -> u32 {
    let byte_offset = byte_offset as usize;
    let prefix = &source[..byte_offset.min(source.len())];
    prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

/// Parse all fallow suppression comments from a file's comment list.
///
/// Supports:
/// - `// fallow-ignore-file` — suppress all issues in the file
/// - `// fallow-ignore-file unused-export` — suppress specific issue type for the file
/// - `// fallow-ignore-next-line` — suppress all issues on the next line
/// - `// fallow-ignore-next-line unused-export` — suppress specific issue type on the next line
#[must_use]
pub fn parse_suppressions(comments: &[Comment], source: &str) -> Vec<Suppression> {
    let mut suppressions = Vec::new();

    for comment in comments {
        let content_span = comment.content_span();
        let text = &source
            [content_span.start as usize..content_span.end.min(source.len() as u32) as usize];
        let trimmed = text.trim();

        if let Some(rest) = trimmed.strip_prefix("fallow-ignore-file") {
            let rest = rest.trim();
            if rest.is_empty() {
                suppressions.push(Suppression {
                    line: 0,
                    kind: None,
                });
            } else if let Some(kind) = IssueKind::parse(rest) {
                suppressions.push(Suppression {
                    line: 0,
                    kind: Some(kind),
                });
            }
            // Unknown kind token: silently ignore (no suppression created)
        } else if let Some(rest) = trimmed.strip_prefix("fallow-ignore-next-line") {
            let rest = rest.trim();
            let comment_line = byte_offset_to_line(source, comment.span.start);
            let suppressed_line = comment_line + 1;

            if rest.is_empty() {
                suppressions.push(Suppression {
                    line: suppressed_line,
                    kind: None,
                });
            } else if let Some(kind) = IssueKind::parse(rest) {
                suppressions.push(Suppression {
                    line: suppressed_line,
                    kind: Some(kind),
                });
            }
            // Unknown kind token: silently ignore
        }
    }

    suppressions
}

/// Parse suppressions from raw source text using simple string scanning.
/// Used for SFC files where comment byte offsets don't correspond to the original file.
pub fn parse_suppressions_from_source(source: &str) -> Vec<Suppression> {
    let mut suppressions = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // Match both // and /* */ style comments
        let comment_text = if let Some(rest) = trimmed.strip_prefix("//") {
            Some(rest.trim())
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            rest.strip_suffix("*/").map(str::trim)
        } else {
            None
        };

        let Some(text) = comment_text else {
            continue;
        };

        if let Some(rest) = text.strip_prefix("fallow-ignore-file") {
            let rest = rest.trim();
            if rest.is_empty() {
                suppressions.push(Suppression {
                    line: 0,
                    kind: None,
                });
            } else if let Some(kind) = IssueKind::parse(rest) {
                suppressions.push(Suppression {
                    line: 0,
                    kind: Some(kind),
                });
            }
        } else if let Some(rest) = text.strip_prefix("fallow-ignore-next-line") {
            let rest = rest.trim();
            let suppressed_line = (line_idx as u32) + 2; // 1-based, next line

            if rest.is_empty() {
                suppressions.push(Suppression {
                    line: suppressed_line,
                    kind: None,
                });
            } else if let Some(kind) = IssueKind::parse(rest) {
                suppressions.push(Suppression {
                    line: suppressed_line,
                    kind: Some(kind),
                });
            }
        }
    }

    suppressions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_wide_suppression() {
        let source = "// fallow-ignore-file\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 0);
        assert!(suppressions[0].kind.is_none());
    }

    #[test]
    fn parse_file_wide_suppression_with_kind() {
        let source = "// fallow-ignore-file unused-export\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 0);
        assert_eq!(suppressions[0].kind, Some(IssueKind::UnusedExport));
    }

    #[test]
    fn parse_next_line_suppression() {
        let source =
            "import { x } from './x';\n// fallow-ignore-next-line\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 3); // suppresses line 3 (the export)
        assert!(suppressions[0].kind.is_none());
    }

    #[test]
    fn parse_next_line_suppression_with_kind() {
        let source = "// fallow-ignore-next-line unused-export\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 2);
        assert_eq!(suppressions[0].kind, Some(IssueKind::UnusedExport));
    }

    #[test]
    fn parse_unknown_kind_ignored() {
        let source = "// fallow-ignore-next-line typo-kind\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert!(suppressions.is_empty());
    }

    #[test]
    fn parse_oxc_comments() {
        use oxc_allocator::Allocator;
        use oxc_parser::Parser;
        use oxc_span::SourceType;

        let source = "// fallow-ignore-file\n// fallow-ignore-next-line unused-export\nexport const foo = 1;\nexport const bar = 2;\n";
        let allocator = Allocator::default();
        let parser_return = Parser::new(&allocator, source, SourceType::mjs()).parse();

        let suppressions = parse_suppressions(&parser_return.program.comments, source);
        assert_eq!(suppressions.len(), 2);

        // File-wide suppression
        assert_eq!(suppressions[0].line, 0);
        assert!(suppressions[0].kind.is_none());

        // Next-line suppression with kind
        assert_eq!(suppressions[1].line, 3); // suppresses line 3 (export const foo)
        assert_eq!(suppressions[1].kind, Some(IssueKind::UnusedExport));
    }

    #[test]
    fn parse_block_comment_suppression() {
        let source = "/* fallow-ignore-file */\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 0);
        assert!(suppressions[0].kind.is_none());
    }

    // ── Additional coverage ─────────────────────────────────────

    #[test]
    fn parse_block_comment_next_line_suppression() {
        let source = "/* fallow-ignore-next-line unused-export */\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 2);
        assert_eq!(suppressions[0].kind, Some(IssueKind::UnusedExport));
    }

    #[test]
    fn parse_multiple_suppressions_on_adjacent_lines() {
        let source = "// fallow-ignore-next-line unused-export\n// fallow-ignore-next-line unused-type\nexport const foo = 1;\nexport type Bar = string;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 2);
        assert_eq!(suppressions[0].line, 2);
        assert_eq!(suppressions[0].kind, Some(IssueKind::UnusedExport));
        assert_eq!(suppressions[1].line, 3);
        assert_eq!(suppressions[1].kind, Some(IssueKind::UnusedType));
    }

    #[test]
    fn parse_file_wide_and_next_line_combined() {
        let source = "// fallow-ignore-file unused-file\n// fallow-ignore-next-line unused-export\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 2);
        assert_eq!(suppressions[0].line, 0);
        assert_eq!(suppressions[0].kind, Some(IssueKind::UnusedFile));
        assert_eq!(suppressions[1].line, 3);
        assert_eq!(suppressions[1].kind, Some(IssueKind::UnusedExport));
    }

    #[test]
    fn parse_suppression_all_issue_kinds() {
        let kinds = [
            ("unused-file", IssueKind::UnusedFile),
            ("unused-export", IssueKind::UnusedExport),
            ("unused-type", IssueKind::UnusedType),
            ("unused-dependency", IssueKind::UnusedDependency),
            ("unused-dev-dependency", IssueKind::UnusedDevDependency),
            ("unused-enum-member", IssueKind::UnusedEnumMember),
            ("unused-class-member", IssueKind::UnusedClassMember),
            ("unresolved-import", IssueKind::UnresolvedImport),
            ("unlisted-dependency", IssueKind::UnlistedDependency),
            ("duplicate-export", IssueKind::DuplicateExport),
            ("code-duplication", IssueKind::CodeDuplication),
            ("circular-dependency", IssueKind::CircularDependency),
        ];
        for (token, expected_kind) in &kinds {
            let source = format!("// fallow-ignore-file {token}\nexport const foo = 1;\n");
            let suppressions = parse_suppressions_from_source(&source);
            assert_eq!(suppressions.len(), 1, "Expected 1 suppression for {token}");
            assert_eq!(suppressions[0].kind, Some(*expected_kind));
        }
    }

    #[test]
    fn parse_block_comment_with_whitespace() {
        let source = "/*  fallow-ignore-file  */\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 0);
        assert!(suppressions[0].kind.is_none());
    }

    #[test]
    fn parse_empty_source_no_suppressions() {
        let suppressions = parse_suppressions_from_source("");
        assert!(suppressions.is_empty());
    }

    #[test]
    fn parse_no_suppression_comments() {
        let source = "// regular comment\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert!(suppressions.is_empty());
    }

    #[test]
    fn parse_suppression_not_at_line_start_ignored() {
        // Inline comments that don't start the line are not parsed
        let source = "export const foo = 1; // fallow-ignore-file\n";
        let suppressions = parse_suppressions_from_source(source);
        assert!(
            suppressions.is_empty(),
            "Inline comment after code should not be parsed as suppression"
        );
    }

    #[test]
    fn parse_block_comment_without_closing_ignored() {
        // A block comment that doesn't end with */ should not be parsed
        let source = "/* fallow-ignore-file\nexport const foo = 1;\n";
        let suppressions = parse_suppressions_from_source(source);
        assert!(suppressions.is_empty());
    }

    #[test]
    fn byte_offset_to_line_first_byte() {
        assert_eq!(byte_offset_to_line("abc\ndef\n", 0), 1);
    }

    #[test]
    fn byte_offset_to_line_second_line() {
        assert_eq!(byte_offset_to_line("abc\ndef\n", 4), 2);
    }

    #[test]
    fn byte_offset_to_line_beyond_source() {
        // Offset beyond source length should be clamped
        assert_eq!(byte_offset_to_line("abc\n", 100), 2);
    }

    #[test]
    fn parse_oxc_block_comment_suppression() {
        use oxc_allocator::Allocator;
        use oxc_parser::Parser;
        use oxc_span::SourceType;

        let source = "/* fallow-ignore-file unused-file */\nexport const foo = 1;\n";
        let allocator = Allocator::default();
        let parser_return = Parser::new(&allocator, source, SourceType::mjs()).parse();

        let suppressions = parse_suppressions(&parser_return.program.comments, source);
        assert_eq!(suppressions.len(), 1);
        assert_eq!(suppressions[0].line, 0);
        assert_eq!(suppressions[0].kind, Some(IssueKind::UnusedFile));
    }

    #[test]
    fn parse_oxc_unknown_kind_ignored() {
        use oxc_allocator::Allocator;
        use oxc_parser::Parser;
        use oxc_span::SourceType;

        let source = "// fallow-ignore-next-line nonexistent-kind\nexport const foo = 1;\n";
        let allocator = Allocator::default();
        let parser_return = Parser::new(&allocator, source, SourceType::mjs()).parse();

        let suppressions = parse_suppressions(&parser_return.program.comments, source);
        assert!(suppressions.is_empty());
    }
}
