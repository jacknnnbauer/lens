//! 导入词典：识别格式 → 校验 → 写数据库 → 建索引 → 加载进注册表。

use std::path::Path;

use super::{index, json, open_dict, stardict, DictKind, DictMeta, DictRegistry};
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
        "ifo" => Ok(DictKind::Stardict),
        "mdx" => Err(Error::UnsupportedFormat("mdx（M1 后续实现）".into())),
        _ => Err(Error::UnsupportedFormat(path.display().to_string())),
    }
}

/// 解析校验完、尚未写库的词典。
enum Prepared {
    Json(json::JsonDictFile),
    StarDict(Box<stardict::Prepared>),
}

impl Prepared {
    fn load(kind: DictKind, path: &Path) -> Result<Self> {
        match kind {
            DictKind::Json => Ok(Self::Json(json::parse(path)?)),
            DictKind::Stardict => Ok(Self::StarDict(Box::new(stardict::prepare(path)?))),
            other => Err(Error::UnsupportedFormat(other.as_str().to_string())),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Json(f) => &f.name,
            Self::StarDict(p) => &p.ifo.bookname,
        }
    }

    fn category(&self) -> Option<String> {
        match self {
            Self::Json(f) => f.category.clone(),
            Self::StarDict(_) => None,
        }
    }

    fn entry_count(&self) -> u64 {
        match self {
            Self::Json(f) => f.entries.len() as u64,
            Self::StarDict(p) => p.entry_count(),
        }
    }

    fn build_index(&self, index_dir: &Path, id: &str, fst: &Path, dup: &Path) -> Result<usize> {
        match self {
            Self::Json(f) => json::build_index(f, fst, dup),
            Self::StarDict(p) => stardict::build_index(p, index_dir, id, fst, dup),
        }
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
    let prepared = Prepared::load(kind, &abs)?;

    let id = uuid::Uuid::new_v4().to_string();
    let (fst_path, dup_path) = index::index_paths(registry.index_dir(), &id);
    let mut row = DictRow {
        id: id.clone(),
        name: prepared.name().to_string(),
        kind: kind.as_str().to_string(),
        path: Some(path_str),
        category: prepared.category(),
        entry_count: Some(prepared.entry_count() as i64),
        index_path: Some(fst_path.to_string_lossy().into_owned()),
        index_state: IndexState::Building,
        created_at: now_ms(),
    };
    db.with_conn(|c| dicts::insert(c, &row))?;

    let built = prepared.build_index(registry.index_dir(), &id, &fst_path, &dup_path).and_then(|keys| {
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

    fn lookup_all(reg: &DictRegistry, id: &str, key: &str) -> Result<Vec<(String, EntryBody)>> {
        let dict = reg.get(id)?.ok_or_else(|| Error::Other("未加载".into()))?;
        Ok(dict.source.lookup(key)?.into_iter().map(|e| (e.headword, e.body)).collect())
    }

    fn text(s: &str) -> EntryBody {
        EntryBody::Text(s.into())
    }

    #[test]
    fn stardict_tm_with_syn() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let meta = import_dict(&db, &reg, &fixture("stardict/roots-tm/roots-tm.ifo"))?;
        assert_eq!((meta.kind, meta.entry_count), (DictKind::Stardict, 7));
        assert_eq!(meta.name, "roots-tm（测试）");

        assert_eq!(
            lookup_all(&reg, &meta.id, "bank")?,
            [("bank".into(), text("[bæŋk]\nn. 河岸")), ("bank".into(), text("[bæŋk]\nn. 银行"))]
        );
        // .syn 同义词命中显示原词头
        assert_eq!(lookup_all(&reg, &meta.id, "Ran")?, [("run".into(), text("[rʌn]\nv. 跑"))]);
        assert_eq!(lookup_all(&reg, &meta.id, "running")?.len(), 1);
        // 大小写不同的两个词头合并为一组，保留各自的原始词头
        let polish: Vec<_> = lookup_all(&reg, &meta.id, "polish")?.into_iter().map(|e| e.0).collect();
        assert_eq!(polish, ["Polish", "polish"]);
        Ok(())
    }

    #[test]
    fn stardict_gz_idx_and_dictzip_across_chunks() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let meta = import_dict(&db, &reg, &fixture("stardict/roots-h-dz/roots-h-dz.ifo"))?;
        assert!(stardict::idx_cache_path(reg.index_dir(), &meta.id).is_file());

        // 这条约 400 字节，跨 64 字节 dictzip 块的多个边界（含被块边界切开的多字节 UTF-8）
        let long = format!(
            "<p>{}</p>",
            (1..=12).map(|i| format!("义项 {i}：在很多场合都会出现的")).collect::<Vec<_>>().join("；")
        );
        assert_eq!(lookup_all(&reg, &meta.id, "omnipresent")?, [("omnipresent".into(), EntryBody::Html(long))]);
        assert_eq!(lookup_all(&reg, &meta.id, "cafe\u{301}")?[0].0, "café");
        assert!(matches!(&lookup_all(&reg, &meta.id, "receive")?[0].1, EntryBody::Html(h) if h.contains("ceive")));

        // 重启后从缓存的 .idx 加载
        let fresh = DictRegistry::new(reg.index_dir().to_path_buf());
        assert_eq!(fresh.load_all(&db)?, 1);
        assert_eq!(lookup_all(&fresh, &meta.id, "ubiquitous")?.len(), 1);
        Ok(())
    }

    #[test]
    fn stardict_typed_fields_without_sametypesequence() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let id = import_dict(&db, &reg, &fixture("stardict/mixed/mixed.ifo"))?.id;
        assert_eq!(lookup_all(&reg, &id, "ubiquitous")?[0].1, text("[juːˈbɪkwɪtəs]\n无处不在的"));
        assert_eq!(
            lookup_all(&reg, &id, "bank")?[0].1,
            EntryBody::Html("<b>bank</b> 银行\n<div>a &amp; b &lt; c</div>".into())
        );
        assert_eq!(lookup_all(&reg, &id, "polish")?[0].1, text("擦亮"));
        // 只有音频字段的词条不产生正文
        assert!(lookup_all(&reg, &id, "sound")?.is_empty());
        Ok(())
    }

    #[test]
    fn stardict_missing_dict_file_is_rejected() -> Result<()> {
        let (dir, db, reg) = setup()?;
        let ifo = dir.path().join("lonely.ifo");
        std::fs::copy(fixture("stardict/roots-tm/roots-tm.ifo"), &ifo)?;
        std::fs::copy(fixture("stardict/roots-tm/roots-tm.idx"), dir.path().join("lonely.idx"))?;
        assert!(matches!(import_dict(&db, &reg, &ifo), Err(Error::InvalidDict(m)) if m.contains(".dict")));
        assert!(db.with_conn(dicts::list)?.is_empty());
        Ok(())
    }
}
