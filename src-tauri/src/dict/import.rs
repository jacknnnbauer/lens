//! 导入词典：识别格式 → 校验 → 写数据库 → 建索引 → 加载进注册表。

use std::path::Path;

use super::{index, json, open_dict, DictKind, DictMeta, DictRegistry};
use crate::db::dicts::{self, DictRow, IndexState};
use crate::db::{now_ms, Db};
use crate::error::{Error, Result};

pub fn detect_kind(path: &Path) -> Result<DictKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "json" => Ok(DictKind::Json),
        "mdx" => Err(Error::UnsupportedFormat("mdx（M1 后续实现）".into())),
        "ifo" => Err(Error::UnsupportedFormat("StarDict（M1 后续实现）".into())),
        _ => Err(Error::UnsupportedFormat(path.display().to_string())),
    }
}

/// 同步执行，调用方负责放到 `spawn_blocking` 里。
pub fn import_dict(db: &Db, registry: &DictRegistry, path: &Path) -> Result<DictMeta> {
    let kind = detect_kind(path)?;
    let abs = std::path::absolute(path)?;
    let path_str = abs.to_string_lossy().into_owned();
    if db.with_conn(|c| dicts::find_id_by_path(c, &path_str))?.is_some() {
        return Err(Error::AlreadyImported(path_str));
    }

    // 先解析校验，失败时数据库里不留任何记录
    let file = match kind {
        DictKind::Json => json::parse(&abs)?,
        other => return Err(Error::UnsupportedFormat(other.as_str().to_string())),
    };

    let id = uuid::Uuid::new_v4().to_string();
    let (fst_path, dup_path) = index::index_paths(registry.index_dir(), &id);
    let mut row = DictRow {
        id: id.clone(),
        name: file.name.clone(),
        kind: kind.as_str().to_string(),
        path: Some(path_str),
        category: file.category.clone(),
        entry_count: Some(file.entries.len() as i64),
        index_path: Some(fst_path.to_string_lossy().into_owned()),
        index_state: IndexState::Building,
        created_at: now_ms(),
    };
    db.with_conn(|c| dicts::insert(c, &row))?;

    let built = json::build_index(&file, &fst_path, &dup_path).and_then(|keys| {
        row.index_state = IndexState::Ready;
        open_dict(&row, registry.index_dir()).map(|d| (keys, d))
    });
    match built {
        Ok((keys, loaded)) => {
            db.with_conn(|c| dicts::set_index_state(c, &id, IndexState::Ready, row.entry_count))?;
            let meta = loaded.source.meta().clone();
            registry.insert(loaded)?;
            tracing::info!(dict = %meta.name, entries = meta.entry_count, keys, "词典导入完成");
            Ok(meta)
        }
        Err(e) => {
            tracing::warn!(dict = %row.name, error = %e, "建索引失败");
            db.with_conn(|c| dicts::set_index_state(c, &id, IndexState::Error, None))?;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::EntryBody;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures").join(name)
    }

    fn setup() -> Result<(tempfile::TempDir, Db, DictRegistry)> {
        let dir = tempfile::tempdir()?;
        let db = Db::open(&dir.path().join("lens.db"))?;
        let registry = DictRegistry::new(dir.path().join("index"));
        std::fs::create_dir_all(registry.index_dir())?;
        Ok((dir, db, registry))
    }

    #[test]
    fn import_build_index_then_lookup() -> Result<()> {
        let (_dir, db, registry) = setup()?;
        let meta = import_dict(&db, &registry, &fixture("json/roots.json"))?;
        assert_eq!(meta.name, "词根词缀（测试）");
        assert_eq!(meta.kind, DictKind::Json);

        let dict = registry.get(&meta.id)?.ok_or_else(|| Error::Other("未加载".into()))?;
        // 别名、大小写都能命中，返回的是条目自己的词头
        let hits = dict.source.lookup("Ubiquity")?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].headword, "ubiquitous");
        assert!(matches!(&hits[0].body, EntryBody::Html(h) if h.contains("ubique")));
        // 同形异义：按文件顺序返回两条
        let banks: Vec<_> = dict.source.lookup("bank")?.into_iter().map(|e| e.body).collect();
        assert_eq!(banks.len(), 2);
        // NFD 输入也能命中 NFC 词头
        assert_eq!(dict.source.lookup("cafe\u{301}")?.len(), 1);
        assert!(dict.source.lookup("nonexistent")?.is_empty());
        // 前缀和模糊走索引
        assert!(dict.index.prefix("bene", 10).iter().any(|(k, _)| k == "benevolent"));
        assert_eq!(dict.index.fuzzy("benevolant", 2, 3)?.first().map(|h| h.0.as_str()), Some("benevolent"));
        Ok(())
    }

    #[test]
    fn imported_dict_is_persisted_and_reloaded() -> Result<()> {
        let (_dir, db, registry) = setup()?;
        let meta = import_dict(&db, &registry, &fixture("json/roots.json"))?;

        let rows = db.with_conn(dicts::list)?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index_state, IndexState::Ready);
        let in_profiles: i64 = db.with_conn(|c| {
            Ok(c.query_row("SELECT COUNT(*) FROM profile_dict WHERE dict_id = ?1", [&meta.id], |r| r.get(0))?)
        })?;
        assert_eq!(in_profiles, 3);

        // 模拟重启：新的注册表从数据库加载
        let fresh = DictRegistry::new(registry.index_dir().to_path_buf());
        assert_eq!(fresh.load_all(&db)?, 1);
        let dict = fresh.get(&meta.id)?.ok_or_else(|| Error::Other("未加载".into()))?;
        assert_eq!(dict.source.lookup("ubiquitous")?.len(), 1);
        Ok(())
    }

    #[test]
    fn rejects_duplicate_and_unsupported_imports() -> Result<()> {
        let (dir, db, registry) = setup()?;
        import_dict(&db, &registry, &fixture("json/roots.json"))?;
        assert!(matches!(
            import_dict(&db, &registry, &fixture("json/roots.json")),
            Err(Error::AlreadyImported(_))
        ));

        let mdx = dir.path().join("x.mdx");
        std::fs::write(&mdx, b"")?;
        assert!(matches!(import_dict(&db, &registry, &mdx), Err(Error::UnsupportedFormat(_))));
        Ok(())
    }

    #[test]
    fn invalid_file_leaves_no_row() -> Result<()> {
        let (dir, db, registry) = setup()?;
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, r#"{"name":"x","version":1,"entries":[{"headword":"","html":""}]}"#)?;
        assert!(matches!(import_dict(&db, &registry, &bad), Err(Error::InvalidDict(_))));
        assert!(db.with_conn(dicts::list)?.is_empty());
        Ok(())
    }

    #[test]
    fn modified_file_is_skipped_on_reload() -> Result<()> {
        let (dir, db, registry) = setup()?;
        let path = dir.path().join("small.json");
        std::fs::write(&path, r#"{"name":"s","version":1,"entries":[{"headword":"a","html":"A"}]}"#)?;
        import_dict(&db, &registry, &path)?;
        std::fs::write(
            &path,
            r#"{"name":"s","version":1,"entries":[{"headword":"a","html":"A"},{"headword":"b","html":"B"}]}"#,
        )?;
        let fresh = DictRegistry::new(registry.index_dir().to_path_buf());
        assert_eq!(fresh.load_all(&db)?, 0);
        Ok(())
    }
}
