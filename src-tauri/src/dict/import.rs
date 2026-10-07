//! 导入词典：识别格式 → 校验 → 写数据库 → 建索引 → 加载进注册表。

use std::path::Path;

use super::{index, json, mdx, open_dict, stardict, DictKind, DictMeta, DictRegistry};
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
        "mdx" => Ok(DictKind::Mdx),
        _ => Err(Error::UnsupportedFormat(path.display().to_string())),
    }
}

/// 解析校验完、尚未写库的词典。
enum Prepared {
    Json(json::JsonDictFile),
    StarDict(Box<stardict::Prepared>),
    Mdx(Box<mdx::Prepared>),
}

impl Prepared {
    fn load(kind: DictKind, path: &Path) -> Result<Self> {
        match kind {
            DictKind::Json => Ok(Self::Json(json::parse(path)?)),
            DictKind::Stardict => Ok(Self::StarDict(Box::new(stardict::prepare(path)?))),
            DictKind::Mdx => Ok(Self::Mdx(Box::new(mdx::prepare(path)?))),
            other => Err(Error::UnsupportedFormat(other.as_str().to_string())),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Json(f) => &f.name,
            Self::StarDict(p) => &p.ifo.bookname,
            Self::Mdx(p) => &p.name,
        }
    }

    fn category(&self) -> Option<String> {
        match self {
            Self::Json(f) => f.category.clone(),
            Self::StarDict(_) | Self::Mdx(_) => None,
        }
    }

    fn entry_count(&self) -> u64 {
        match self {
            Self::Json(f) => f.entries.len() as u64,
            Self::StarDict(p) => p.entry_count(),
            Self::Mdx(p) => p.entry_count(),
        }
    }

    fn build_index(&self, index_dir: &Path, id: &str, fst: &Path, dup: &Path) -> Result<usize> {
        match self {
            Self::Json(f) => json::build_index(f, fst, dup),
            Self::StarDict(p) => stardict::build_index(p, index_dir, id, fst, dup),
            Self::Mdx(p) => mdx::build_index(p, index_dir, id),
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

/// 删除词典：数据库记录、注册表、索引目录下的 `<id>.*` 文件。
/// 文件删除失败（例如仍被映射）只记日志，下次启动时由 `sweep_orphans` 清理。
pub fn remove_dict(db: &Db, registry: &DictRegistry, id: &str) -> Result<bool> {
    let existed = db.with_conn(|c| dicts::delete(c, id))?;
    drop(registry.remove(id)?);
    delete_index_files(registry.index_dir(), id);
    Ok(existed)
}

fn delete_index_files(dir: &Path, id: &str) {
    let prefix = format!("{id}.");
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) {
            if let Err(err) = std::fs::remove_file(e.path()) {
                tracing::warn!(file = %e.path().display(), error = %err, "删除索引文件失败");
            }
        }
    }
}

/// 启动时清理索引目录里不属于任何词典的文件（删除词典时没删掉的残留）。
pub fn sweep_orphans(db: &Db, index_dir: &Path) -> Result<usize> {
    let ids: std::collections::HashSet<String> =
        db.with_conn(dicts::list)?.into_iter().map(|r| r.id).collect();
    let mut removed = 0;
    for e in std::fs::read_dir(index_dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let id = name.split('.').next().unwrap_or_default();
        if !ids.contains(id) && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
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
    fn remove_deletes_row_registry_and_files() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let id = import_dict(&db, &reg, &fixture("mdx/basic/basic.mdx"))?.id;
        let count = || -> Result<usize> {
            Ok(std::fs::read_dir(reg.index_dir())?
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(&id))
                .count())
        };
        assert!(count()? >= 9, "主词典 + 两个 mdd 各 3 个文件");
        assert!(remove_dict(&db, &reg, &id)?);
        assert!(reg.get(&id)?.is_none());
        assert!(db.with_conn(dicts::list)?.is_empty());
        assert_eq!(count()?, 0);
        assert!(!remove_dict(&db, &reg, &id)?);

        // 残留文件在启动清理时被删掉
        std::fs::write(reg.index_dir().join("dead-beef.fst"), b"x")?;
        assert_eq!(sweep_orphans(&db, reg.index_dir())?, 1);
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

        let bgl = dir.path().join("x.bgl");
        std::fs::write(&bgl, b"")?;
        assert!(matches!(import_dict(&db, &registry, &bgl), Err(Error::UnsupportedFormat(_))));
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

        // res/ 目录资源；不允许越出 res/
        let dict = reg.get(&meta.id)?.ok_or_else(|| Error::Other("未加载".into()))?;
        assert_eq!(dict.source.resource("img/root.gif")?.as_deref(), Some(&b"GIF89a-fake"[..]));
        assert_eq!(dict.source.resource("../roots-tm.ifo")?, None);
        assert_eq!(dict.source.resource("img")?, None);
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

    /// 真实词典兼容性 / 性能检查（只读原文件，索引建在临时目录）：
    /// `LENS_REAL_DICTS="a.mdx;b.mdx" cargo test --release real_dicts -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_dicts() -> Result<()> {
        use std::time::{Duration, Instant};
        let Ok(list) = std::env::var("LENS_REAL_DICTS") else {
            println!("未设置 LENS_REAL_DICTS，跳过");
            return Ok(());
        };
        let probes = ["ubiquitous", "run", "ran", "colour", "bank", "take off", "résumé", "Polish", "set"];
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let mut failed = Vec::new();

        for path in list.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            let size_mb = std::fs::metadata(&path).map(|m| m.len() as f64 / 1e6).unwrap_or(0.0);
            let (_dir, db, reg) = setup()?;
            let t = Instant::now();
            let meta = match import_dict(&db, &reg, &path) {
                Ok(m) => m,
                Err(e) => {
                    println!("✗ {} ({size_mb:.0} MB)：{e}", path.display());
                    failed.push(path.display().to_string());
                    continue;
                }
            };
            let import = t.elapsed();

            let t = Instant::now();
            let fresh = DictRegistry::new(reg.index_dir().to_path_buf());
            fresh.load_all(&db)?;
            let open = t.elapsed();
            let dict = fresh.get(&meta.id)?.ok_or_else(|| Error::Other("未加载".into()))?;

            // 每个首字母取一批真实词头做计时样本
            let mut sample: Vec<String> = probes.iter().map(|s| s.to_string()).collect();
            for c in 'a'..='z' {
                sample.extend(dict.index.prefix(&c.to_string(), 40).into_iter().map(|(k, _)| k));
            }
            let mut times = Vec::with_capacity(sample.len());
            let mut errors = 0;
            for w in &sample {
                let t = Instant::now();
                if let Err(e) = dict.source.lookup(w) {
                    errors += 1;
                    if errors <= 3 {
                        println!("    查 {w:?} 出错：{e}");
                    }
                }
                times.push(t.elapsed());
            }
            times.sort();
            let p = |q: usize| ms(times[(times.len() * q / 100).min(times.len() - 1)]);

            println!(
                "✓ {} | {size_mb:.0} MB | {} 条 | 导入 {:.0} ms | 冷打开 {:.1} ms | 查词 n={} p50 {:.2} ms p95 {:.2} ms max {:.1} ms | 错误 {errors}",
                meta.name,
                meta.entry_count,
                ms(import),
                ms(open),
                times.len(),
                p(50),
                p(95),
                ms(times[times.len() - 1]),
            );
            for w in ["ubiquitous", "ran", "colour"] {
                let hits = dict.source.lookup(w)?;
                let preview: String = hits
                    .first()
                    .map(|e| match &e.body {
                        EntryBody::Html(s) | EntryBody::Text(s) => s.chars().take(90).collect(),
                    })
                    .unwrap_or_default();
                println!("    {w} → {} 条 {:?} {}", hits.len(), hits.first().map(|e| &e.headword), preview.replace('\n', " "));
            }
            // mdd 资源：取词条里引用的本地 css / 图片
            let mut refs = Vec::new();
            for w in ["ubiquitous", "colour", "bank"] {
                for e in dict.source.lookup(w)? {
                    let EntryBody::Html(h) = e.body else { continue };
                    for attr in ["href=\"", "src=\""] {
                        for part in h.split(attr).skip(1) {
                            let r = part.split('"').next().unwrap_or_default();
                            if !r.contains("://") && !r.is_empty() && !refs.iter().any(|x: &String| x == r) {
                                refs.push(r.to_string());
                            }
                        }
                    }
                }
            }
            let (mut found, mut missing) = (Vec::new(), Vec::new());
            for r in refs.iter().take(8) {
                match dict.source.resource(r)? {
                    Some(b) => found.push(format!("{r} ({} B)", b.len())),
                    None => missing.push(r.clone()),
                }
            }
            if !refs.is_empty() {
                println!("    资源 命中 {:?} | 未找到 {:?}", found, missing);
            }
        }
        if !failed.is_empty() {
            println!("失败 {} 部：{failed:?}", failed.len());
        }
        Ok(())
    }

    fn html(s: &str) -> EntryBody {
        EntryBody::Html(s.into())
    }

    #[test]
    fn mdx_v2_encrypted_info_stylesheet_links_and_mdd() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let meta = import_dict(&db, &reg, &fixture("mdx/basic/basic.mdx"))?;
        assert_eq!((meta.kind, meta.entry_count), (DictKind::Mdx, 13));
        assert_eq!(meta.name, "Basic（测试）");
        let id = &meta.id;

        // StyleSheet 替换
        assert_eq!(
            lookup_all(&reg, id, "ubiquitous")?,
            [("ubiquitous".into(), html("<b>ubiquitous</b><i>adj. 无处不在的</i>"))]
        );
        // @@@LINK 跟随，显示目标词头
        assert_eq!(lookup_all(&reg, id, "ubiquity")?[0].0, "ubiquitous");
        // Colour → colour → color 两层链接；colour/Colour 同组，最终同一条记录只返回一次
        let colour = lookup_all(&reg, id, "COLOUR")?;
        assert_eq!(colour.len(), 1);
        assert_eq!(colour[0].0, "color");
        // 环形链接不死循环、不返回内容
        assert!(lookup_all(&reg, id, "loop1")?.is_empty());
        // 同形异义两条；大小写不同的词头各自保留
        assert_eq!(lookup_all(&reg, id, "bank")?.len(), 2);
        let polish: Vec<_> = lookup_all(&reg, id, "polish")?.into_iter().map(|e| e.0).collect();
        assert_eq!(polish, ["Polish", "polish"]);
        assert_eq!(lookup_all(&reg, id, "cafe\u{301}")?[0].0, "café");
        // 跨多个记录块的长词条
        let long = format!(
            "<p>{}</p>",
            (1..=8).map(|i| format!("义项 {i}：在很多场合都会出现的")).collect::<Vec<_>>().join("；")
        );
        assert_eq!(lookup_all(&reg, id, "omnipresent")?[0].1, html(&long));

        // mdd 资源：分隔符、大小写、多个 mdd 文件
        let dict = reg.get(id)?.ok_or_else(|| Error::Other("未加载".into()))?;
        assert_eq!(dict.source.resource("img/dot.png")?.as_deref(), Some(&b"\x89PNG\r\n\x1a\nfake"[..]));
        // 同目录散装文件优先于 mdd 里的同名资源；带查询串也能找到
        assert_eq!(dict.source.resource("\\STYLE.CSS")?.as_deref(), Some(&b"/* loose overrides mdd */"[..]));
        assert_eq!(dict.source.resource("basic.css?v=2")?.as_deref(), Some(&b".hw { font-weight: bold }"[..]));
        // 不允许越出词典目录
        for evil in ["../legacy/legacy.mdx", "..\\utf16\\utf16.mdx", "C:/Windows/win.ini", "", "/"] {
            assert_eq!(dict.source.resource(evil)?, None, "{evil}");
        }
        assert_eq!(dict.source.resource("/ubiquitous.mp3")?.as_deref(), Some(&b"ID3fake-audio"[..]));
        assert_eq!(dict.source.resource("missing.png")?, None);

        // 重启后加载
        let fresh = DictRegistry::new(reg.index_dir().to_path_buf());
        assert_eq!(fresh.load_all(&db)?, 1);
        assert_eq!(lookup_all(&fresh, id, "bank")?.len(), 2);
        Ok(())
    }

    #[test]
    fn mdx_v1_gbk_lzo() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let meta = import_dict(&db, &reg, &fixture("mdx/legacy/legacy.mdx"))?;
        assert_eq!(meta.entry_count, 5);
        assert_eq!(lookup_all(&reg, &meta.id, "中国")?, [("中国".into(), html("<p>China</p>"))]);
        assert_eq!(lookup_all(&reg, &meta.id, "中文")?[0].0, "汉语");
        assert_eq!(lookup_all(&reg, &meta.id, "Abandon")?[0].1, html("<p>放弃；抛弃</p>"));
        Ok(())
    }

    #[test]
    fn mdx_utf16_uncompressed_and_title_fallback() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        let meta = import_dict(&db, &reg, &fixture("mdx/utf16/utf16.mdx"))?;
        assert_eq!(meta.name, "utf16");
        assert_eq!(lookup_all(&reg, &meta.id, "naïve")?[0].1, html("<p>天真的</p>"));
        assert_eq!(lookup_all(&reg, &meta.id, "world")?[0].1, html("<p>世界</p>"));
        Ok(())
    }

    #[test]
    fn mdx_unsupported_variants_have_clear_errors() -> Result<()> {
        let (_dir, db, reg) = setup()?;
        assert!(matches!(
            import_dict(&db, &reg, &fixture("mdx/regcode.mdx")),
            Err(Error::InvalidDict(m)) if m.contains("授权")
        ));
        assert!(matches!(
            import_dict(&db, &reg, &fixture("mdx/v3.mdx")),
            Err(Error::UnsupportedFormat(m)) if m.contains("3.0")
        ));
        assert!(db.with_conn(dicts::list)?.is_empty());
        Ok(())
    }
}
