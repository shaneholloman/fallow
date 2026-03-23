//! Tests for the incremental parse cache.

use std::path::Path;

use oxc_span::Span;

use crate::*;
use fallow_types::discover::FileId;

use super::*;

#[test]
fn cache_store_new_is_empty() {
    let store = CacheStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn cache_store_default_is_empty() {
    let store = CacheStore::default();
    assert!(store.is_empty());
}

#[test]
fn cache_store_insert_and_get() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);
    assert_eq!(store.len(), 1);
    assert!(!store.is_empty());
    assert!(store.get(Path::new("test.ts"), 42).is_some());
}

#[test]
fn cache_store_hash_mismatch_returns_none() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);
    assert!(store.get(Path::new("test.ts"), 99).is_none());
}

#[test]
fn cache_store_missing_key_returns_none() {
    let store = CacheStore::new();
    assert!(store.get(Path::new("nonexistent.ts"), 42).is_none());
}

#[test]
fn cache_store_overwrite_entry() {
    let mut store = CacheStore::new();
    let m1 = CachedModule {
        content_hash: 1,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    let m2 = CachedModule {
        content_hash: 2,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), m1);
    store.insert(Path::new("test.ts"), m2);
    assert_eq!(store.len(), 1);
    assert!(store.get(Path::new("test.ts"), 1).is_none());
    assert!(store.get(Path::new("test.ts"), 2).is_some());
}

#[test]
fn module_to_cached_roundtrip_named_export() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![ExportInfo {
            name: ExportName::Named("foo".to_string()),
            local_name: Some("foo".to_string()),
            is_type_only: false,
            is_public: false,
            span: Span::new(10, 20),
            members: vec![],
        }],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 123,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert_eq!(restored.exports.len(), 1);
    assert_eq!(
        restored.exports[0].name,
        ExportName::Named("foo".to_string())
    );
    assert!(!restored.exports[0].is_type_only);
    assert_eq!(restored.exports[0].span.start, 10);
    assert_eq!(restored.exports[0].span.end, 20);
    assert_eq!(restored.content_hash, 123);
}

#[test]
fn module_to_cached_roundtrip_default_export() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![ExportInfo {
            name: ExportName::Default,
            local_name: None,
            is_type_only: false,
            is_public: false,
            span: Span::new(0, 10),
            members: vec![],
        }],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 456,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert_eq!(restored.exports[0].name, ExportName::Default);
}

#[test]
fn module_to_cached_roundtrip_imports() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![],
        imports: vec![
            ImportInfo {
                source: "./utils".to_string(),
                imported_name: ImportedName::Named("foo".to_string()),
                local_name: "foo".to_string(),
                is_type_only: false,
                span: Span::new(0, 10),
            },
            ImportInfo {
                source: "react".to_string(),
                imported_name: ImportedName::Default,
                local_name: "React".to_string(),
                is_type_only: false,
                span: Span::new(15, 30),
            },
            ImportInfo {
                source: "./all".to_string(),
                imported_name: ImportedName::Namespace,
                local_name: "all".to_string(),
                is_type_only: false,
                span: Span::new(35, 50),
            },
            ImportInfo {
                source: "./styles.css".to_string(),
                imported_name: ImportedName::SideEffect,
                local_name: String::new(),
                is_type_only: false,
                span: Span::new(55, 70),
            },
        ],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 789,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert_eq!(restored.imports.len(), 4);
    assert_eq!(
        restored.imports[0].imported_name,
        ImportedName::Named("foo".to_string())
    );
    assert_eq!(restored.imports[0].span.start, 0);
    assert_eq!(restored.imports[0].span.end, 10);
    assert_eq!(restored.imports[1].imported_name, ImportedName::Default);
    assert_eq!(restored.imports[1].span.start, 15);
    assert_eq!(restored.imports[1].span.end, 30);
    assert_eq!(restored.imports[2].imported_name, ImportedName::Namespace);
    assert_eq!(restored.imports[2].span.start, 35);
    assert_eq!(restored.imports[2].span.end, 50);
    assert_eq!(restored.imports[3].imported_name, ImportedName::SideEffect);
    assert_eq!(restored.imports[3].span.start, 55);
    assert_eq!(restored.imports[3].span.end, 70);
}

#[test]
fn module_to_cached_roundtrip_re_exports() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![],
        imports: vec![],
        re_exports: vec![ReExportInfo {
            source: "./module".to_string(),
            imported_name: "foo".to_string(),
            exported_name: "bar".to_string(),
            is_type_only: true,
        }],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 0,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert_eq!(restored.re_exports.len(), 1);
    assert_eq!(restored.re_exports[0].source, "./module");
    assert_eq!(restored.re_exports[0].imported_name, "foo");
    assert_eq!(restored.re_exports[0].exported_name, "bar");
    assert!(restored.re_exports[0].is_type_only);
}

#[test]
fn module_to_cached_roundtrip_dynamic_imports() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![DynamicImportInfo {
            source: "./lazy".to_string(),
            span: Span::new(0, 10),
            destructured_names: Vec::new(),
            local_name: None,
        }],
        require_calls: vec![RequireCallInfo {
            source: "fs".to_string(),
            span: Span::new(15, 25),
            destructured_names: Vec::new(),
            local_name: None,
        }],
        member_accesses: vec![MemberAccess {
            object: "Status".to_string(),
            member: "Active".to_string(),
        }],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: true,
        content_hash: 0,
        suppressions: vec![],
        unused_import_bindings: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert_eq!(restored.dynamic_imports.len(), 1);
    assert_eq!(restored.dynamic_imports[0].source, "./lazy");
    assert_eq!(restored.dynamic_imports[0].span.start, 0);
    assert_eq!(restored.dynamic_imports[0].span.end, 10);
    assert_eq!(restored.require_calls.len(), 1);
    assert_eq!(restored.require_calls[0].source, "fs");
    assert_eq!(restored.require_calls[0].span.start, 15);
    assert_eq!(restored.require_calls[0].span.end, 25);
    assert_eq!(restored.member_accesses.len(), 1);
    assert_eq!(restored.member_accesses[0].object, "Status");
    assert_eq!(restored.member_accesses[0].member, "Active");
    assert!(restored.has_cjs_exports);
}

#[test]
fn module_to_cached_roundtrip_members() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![ExportInfo {
            name: ExportName::Named("Color".to_string()),
            local_name: Some("Color".to_string()),
            is_type_only: false,
            is_public: false,
            span: Span::new(0, 50),
            members: vec![
                MemberInfo {
                    name: "Red".to_string(),
                    kind: MemberKind::EnumMember,
                    span: Span::new(10, 15),
                    has_decorator: false,
                },
                MemberInfo {
                    name: "greet".to_string(),
                    kind: MemberKind::ClassMethod,
                    span: Span::new(20, 30),
                    has_decorator: false,
                },
                MemberInfo {
                    name: "name".to_string(),
                    kind: MemberKind::ClassProperty,
                    span: Span::new(35, 45),
                    has_decorator: false,
                },
            ],
        }],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 0,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert_eq!(restored.exports[0].members.len(), 3);
    assert_eq!(restored.exports[0].members[0].kind, MemberKind::EnumMember);
    assert_eq!(restored.exports[0].members[1].kind, MemberKind::ClassMethod);
    assert_eq!(
        restored.exports[0].members[2].kind,
        MemberKind::ClassProperty
    );
}

#[test]
fn cache_load_nonexistent_returns_none() {
    let result = CacheStore::load(Path::new("/nonexistent/path"));
    assert!(result.is_none());
}

/// Create a unique temporary directory for cache tests.
fn test_cache_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir()
        .join("fallow_cache_tests")
        .join(name)
        .join(format!("{}", std::process::id()));
    // Clean up any leftover from previous runs
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cache_save_and_load_roundtrip() {
    let dir = test_cache_dir("roundtrip");
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);
    store.save(&dir).unwrap();

    let loaded = CacheStore::load(&dir);
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded.get(Path::new("test.ts"), 42).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_version_mismatch_returns_none() {
    let dir = test_cache_dir("version_mismatch");
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);
    store.save(&dir).unwrap();

    // Verify the cache loads correctly before tampering
    assert!(CacheStore::load(&dir).is_some());

    // Read raw bytes and modify the version field.
    // With bincode standard config, u32 is varint-encoded.
    // The version (CACHE_VERSION) is the first encoded field.
    // Replace the first byte with a different version value (e.g., 255)
    // to simulate a version mismatch.
    let cache_file = dir.join("cache.bin");
    let mut data = std::fs::read(&cache_file).unwrap();
    assert!(!data.is_empty());
    data[0] = 255; // Corrupt the version byte
    std::fs::write(&cache_file, &data).unwrap();

    // Loading should return None due to version mismatch
    let result = CacheStore::load(&dir);
    assert!(result.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn module_to_cached_roundtrip_type_only_import() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![],
        imports: vec![ImportInfo {
            source: "./types".to_string(),
            imported_name: ImportedName::Named("Foo".to_string()),
            local_name: "Foo".to_string(),
            is_type_only: true,
            span: Span::new(0, 10),
        }],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 0,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));

    assert!(restored.imports[0].is_type_only);
    assert_eq!(restored.imports[0].span.start, 0);
    assert_eq!(restored.imports[0].span.end, 10);
}

#[test]
fn get_by_path_only_returns_entry_regardless_of_hash() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);

    // get_by_path_only should return the entry without checking hash
    let result = store.get_by_path_only(Path::new("test.ts"));
    assert!(result.is_some());
    assert_eq!(result.unwrap().content_hash, 42);
}

#[test]
fn get_by_path_only_returns_none_for_missing() {
    let store = CacheStore::new();
    assert!(
        store
            .get_by_path_only(Path::new("nonexistent.ts"))
            .is_none()
    );
}

#[test]
fn retain_paths_removes_stale_entries() {
    use fallow_types::discover::DiscoveredFile;
    use std::path::PathBuf;

    let mut store = CacheStore::new();
    let m = || CachedModule {
        content_hash: 1,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };

    store.insert(Path::new("/project/a.ts"), m());
    store.insert(Path::new("/project/b.ts"), m());
    store.insert(Path::new("/project/c.ts"), m());
    assert_eq!(store.len(), 3);

    // Only a.ts and c.ts still exist in the project
    let files = vec![
        DiscoveredFile {
            id: FileId(0),
            path: PathBuf::from("/project/a.ts"),
            size_bytes: 100,
        },
        DiscoveredFile {
            id: FileId(1),
            path: PathBuf::from("/project/c.ts"),
            size_bytes: 50,
        },
    ];

    store.retain_paths(&files);
    assert_eq!(store.len(), 2);
    assert!(store.get_by_path_only(Path::new("/project/a.ts")).is_some());
    assert!(store.get_by_path_only(Path::new("/project/b.ts")).is_none());
    assert!(store.get_by_path_only(Path::new("/project/c.ts")).is_some());
}

#[test]
fn retain_paths_with_empty_files_clears_cache() {
    let mut store = CacheStore::new();
    let m = CachedModule {
        content_hash: 1,
        mtime_secs: 0,
        file_size: 0,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("a.ts"), m);
    assert_eq!(store.len(), 1);

    store.retain_paths(&[]);
    assert!(store.is_empty());
}

#[test]
fn get_by_metadata_returns_entry_on_match() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 1000,
        file_size: 500,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);

    let result = store.get_by_metadata(Path::new("test.ts"), 1000, 500);
    assert!(result.is_some());
    assert_eq!(result.unwrap().content_hash, 42);
}

#[test]
fn get_by_metadata_returns_none_on_mtime_mismatch() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 1000,
        file_size: 500,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);

    assert!(
        store
            .get_by_metadata(Path::new("test.ts"), 2000, 500)
            .is_none()
    );
}

#[test]
fn get_by_metadata_returns_none_on_size_mismatch() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 1000,
        file_size: 500,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);

    assert!(
        store
            .get_by_metadata(Path::new("test.ts"), 1000, 999)
            .is_none()
    );
}

#[test]
fn get_by_metadata_returns_none_for_zero_mtime() {
    let mut store = CacheStore::new();
    let module = CachedModule {
        content_hash: 42,
        mtime_secs: 0,
        file_size: 500,
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        suppressions: vec![],
        line_offsets: vec![],
    };
    store.insert(Path::new("test.ts"), module);

    // Zero mtime should never match (falls through to content hash check)
    assert!(
        store
            .get_by_metadata(Path::new("test.ts"), 0, 500)
            .is_none()
    );
}

#[test]
fn get_by_metadata_returns_none_for_missing_file() {
    let store = CacheStore::new();
    assert!(
        store
            .get_by_metadata(Path::new("nonexistent.ts"), 1000, 500)
            .is_none()
    );
}

#[test]
fn module_to_cached_stores_mtime_and_size() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 42,
        suppressions: vec![],
        line_offsets: vec![],
    };

    let cached = module_to_cached(&module, 12345, 6789);
    assert_eq!(cached.mtime_secs, 12345);
    assert_eq!(cached.file_size, 6789);
    assert_eq!(cached.content_hash, 42);
}

#[test]
fn module_to_cached_roundtrip_line_offsets() {
    let module = ModuleInfo {
        file_id: FileId(0),
        exports: vec![],
        imports: vec![],
        re_exports: vec![],
        dynamic_imports: vec![],
        require_calls: vec![],
        member_accesses: vec![],
        whole_object_uses: vec![],
        dynamic_import_patterns: vec![],
        has_cjs_exports: false,
        unused_import_bindings: vec![],
        content_hash: 0,
        suppressions: vec![],
        line_offsets: vec![0, 15, 30, 45],
    };
    let cached = module_to_cached(&module, 0, 0);
    let restored = cached_to_module(&cached, FileId(0));
    assert_eq!(restored.line_offsets, vec![0, 15, 30, 45]);
}
