//! Shared graph types: module nodes, re-export edges, export symbols, and references.

use std::ops::Range;
use std::path::PathBuf;

use fallow_types::discover::FileId;
use fallow_types::extract::ExportName;

/// A single module in the graph.
#[derive(Debug)]
pub struct ModuleNode {
    /// Unique identifier for this module.
    pub file_id: FileId,
    /// Absolute path to the module file.
    pub path: PathBuf,
    /// Range into the flat `edges` array.
    pub edge_range: Range<usize>,
    /// Exports declared by this module.
    pub exports: Vec<ExportSymbol>,
    /// Re-exports from this module (export { x } from './y', export * from './z').
    pub re_exports: Vec<ReExportEdge>,
    /// Whether this module is an entry point.
    pub is_entry_point: bool,
    /// Whether this module is reachable from any entry point.
    pub is_reachable: bool,
    /// Whether this module has CJS exports (module.exports / exports.*).
    pub has_cjs_exports: bool,
}

/// A re-export edge, tracking which exports are forwarded from which module.
#[derive(Debug)]
pub struct ReExportEdge {
    /// The module being re-exported from.
    pub source_file: FileId,
    /// The name imported from the source (or "*" for star re-exports).
    pub imported_name: String,
    /// The name exported from this module.
    pub exported_name: String,
    /// Whether this is a type-only re-export.
    pub is_type_only: bool,
}

/// An export with reference tracking.
#[derive(Debug)]
pub struct ExportSymbol {
    /// The exported name (named or default).
    pub name: ExportName,
    /// Whether this is a type-only export.
    pub is_type_only: bool,
    /// Source span of the export declaration.
    pub span: oxc_span::Span,
    /// Which files reference this export.
    pub references: Vec<SymbolReference>,
    /// Members of this export (enum members, class members).
    pub members: Vec<fallow_types::extract::MemberInfo>,
}

/// A reference to an export from another file.
#[derive(Debug, Clone)]
pub struct SymbolReference {
    /// The file that references this export.
    pub from_file: FileId,
    /// How the export is referenced.
    pub kind: ReferenceKind,
    /// Byte span of the import statement in the referencing file.
    /// Used by the LSP to locate references for Code Lens navigation.
    pub import_span: oxc_span::Span,
}

/// How an export is referenced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A named import (`import { foo }`).
    NamedImport,
    /// A default import (`import Foo`).
    DefaultImport,
    /// A namespace import (`import * as ns`).
    NamespaceImport,
    /// A re-export (`export { foo } from './bar'`).
    ReExport,
    /// A dynamic import (`import('./foo')`).
    DynamicImport,
    /// A side-effect import (`import './styles'`).
    SideEffectImport,
}

// Size assertions for types defined in this module.
// `ExportSymbol` and `SymbolReference` are stored in Vecs per module node.
// `ReExportEdge` is stored in a Vec per module for re-export chain resolution.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<ExportSymbol>() == 88);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<SymbolReference>() == 16);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<ReExportEdge>() == 56);
// `ModuleNode` is stored in a Vec — one per discovered file.
// PathBuf has different sizes on Unix vs Windows, so restrict to Unix.
#[cfg(all(target_pointer_width = "64", unix))]
const _: () = assert!(std::mem::size_of::<ModuleNode>() == 96);
