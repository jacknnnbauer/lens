//! 内置英汉词典（由 ECDICT 裁剪，`scripts/build_builtin_dict.py` 生成）：全新安装时解压并导入。

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};

use crate::app_state::AppState;
use crate::db::Db;
use crate::dict::{import, DictMeta, DictRegistry};
use crate::error::{Error, Result};

pub const IFO_NAME: &str = "lens-ecdict.ifo";
/// 解压到数据目录下的这个文件夹
const DIR: &str = "builtin";

// 手机版暂时不带内置词典
#[cfg(not(target_os = "android"))]
const DATA: &[u8] = include_bytes!("../data/lens-ecdict.zip");
#[cfg(target_os = "android")]
const DATA: &[u8] = &[];

/// 词典库是空的、用户也没删过内置词典，才自动安装
pub fn should_install(dict_count: usize, removed: bool) -> bool {
    dict_count == 0 && !removed
}

/// 解压到 `dir`，返回 .ifo 路径。先解到临时文件夹再改名：中途退出不会留下半套文件
pub fn extract_to(dir: &Path) -> Result<PathBuf> {
    let ifo = dir.join(IFO_NAME);
    if ifo.is_file() {
        return Ok(ifo);
    }
    let tmp = dir.with_extension("tmp");
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir_all(&tmp)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(DATA)).map_err(|e| Error::Other(e.to_string()))?;
    zip.extract(&tmp).map_err(|e| Error::Other(e.to_string()))?;
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::rename(&tmp, dir)?;
    Ok(ifo)
}

/// 解压并导入；已经导入过时返回 None
pub fn install_into(db: &Db, registry: &DictRegistry, root: &Path) -> Result<Option<DictMeta>> {
    if DATA.is_empty() {
        return Ok(None);
    }
    let ifo = extract_to(&root.join(DIR))?;
    match import::import_dict(db, registry, &ifo) {
        Ok(meta) => Ok(Some(meta)),
        Err(Error::AlreadyImported(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// 启动时调用：全新安装自动加入内置词典
pub fn install(state: &AppState) -> Result<Option<DictMeta>> {
    let count: i64 = state.db.with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM dict", [], |r| r.get(0))?))?;
    if !should_install(count as usize, state.settings.get()?.general.builtin_dict_removed) {
        return Ok(None);
    }
    install_into(&state.db, &state.dicts, &state.root)
}

/// 这部词典是不是内置词典
pub fn is_builtin(db: &Db, root: &Path, id: &str) -> Result<bool> {
    let path: Option<Option<String>> =
        db.with_conn(|c| Ok(c.query_row("SELECT path FROM dict WHERE id = ?1", params![id], |r| r.get(0)).optional()?))?;
    let dir = std::path::absolute(root.join(DIR))?;
    Ok(path.flatten().is_some_and(|p| Path::new(&p).starts_with(&dir)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::dict::{DictRegistry, EntryBody};

    #[test]
    fn installs_only_when_library_empty_and_not_removed() {
        assert!(should_install(0, false));
        assert!(!should_install(1, false));
        assert!(!should_install(0, true));
    }

    #[test]
    fn extract_is_repeatable_after_partial_leftovers() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("builtin");
        std::fs::create_dir_all(d.path().join("builtin.tmp")).unwrap();
        std::fs::write(d.path().join("builtin.tmp/junk"), b"x").unwrap();
        let ifo = extract_to(&target).unwrap();
        assert!(ifo.ends_with(IFO_NAME) && ifo.exists());
        assert!(!target.join("junk").exists(), "上次中断留下的文件不能带进来");
        assert!(extract_to(&target).unwrap().exists());
    }

    #[test]
    fn embedded_dictionary_imports_and_finds_common_words() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(&d.path().join("lens.db")).unwrap();
        let reg = DictRegistry::new(d.path().join("index"));
        std::fs::create_dir_all(reg.index_dir()).unwrap();
        let meta = install_into(&db, &reg, d.path()).unwrap().expect("空词典库应该安装");
        let dict = reg.get(&meta.id).unwrap().unwrap();
        let hits = dict.source.lookup("house").unwrap();
        assert!(matches!(&hits[0].body, EntryBody::Html(h) if h.contains("房子")));
        assert_eq!(dict.source.lookup("went").unwrap()[0].headword, "go", "变形经 .syn 指向原形");
        assert!(is_builtin(&db, d.path(), &meta.id).unwrap());
    }

    #[test]
    fn second_install_does_not_duplicate() {
        let d = tempfile::tempdir().unwrap();
        let db = Db::open(&d.path().join("lens.db")).unwrap();
        let reg = DictRegistry::new(d.path().join("index"));
        std::fs::create_dir_all(reg.index_dir()).unwrap();
        install_into(&db, &reg, d.path()).unwrap();
        assert!(install_into(&db, &reg, d.path()).unwrap().is_none());
        assert_eq!(reg.len().unwrap(), 1);
    }
}
