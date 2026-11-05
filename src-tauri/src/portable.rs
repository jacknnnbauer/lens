//! 便携版：exe 旁边有 `portable.txt` 时，所有数据都放在 exe 旁边的 `data` 文件夹，
//! 不写 `%APPDATA%`，也不和已安装的 Lens 共用单实例。同一个程序，靠这个文件区分。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tauri::{Manager, Runtime, WebviewWindowBuilder};

use crate::db::{dicts, Db};
use crate::error::Result;

const MARKER: &str = "portable.txt";
/// 记下上次运行时便携文件夹的位置，用来发现文件夹被挪走或 U 盘换了盘符
const BASE_FILE: &str = "portable-base.txt";

/// 便携文件夹（exe 所在目录）；不是便携版时为 None。
/// 开发时用 LENS_DATA_DIR 指定了数据目录的不算。
pub fn base() -> Option<&'static Path> {
    static BASE: OnceLock<Option<PathBuf>> = OnceLock::new();
    BASE.get_or_init(|| {
        if std::env::var_os("LENS_DATA_DIR").is_some() {
            return None;
        }
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        dir.join(MARKER).is_file().then(|| dir.to_path_buf())
    })
    .as_deref()
}

pub fn data_dir(base: &Path) -> PathBuf {
    base.join("data")
}

/// 单实例按这个标识区分：不同位置的便携版、已安装的 Lens 互不干扰
pub fn identifier(base: &Path) -> String {
    let hash = base.to_string_lossy().bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("com.lens.app.portable.p{hash:016x}")
}

/// 建好数据目录。必须在创建任何窗口之前调用。
pub fn prepare(base: &Path) -> Result<()> {
    let data = data_dir(base);
    std::fs::create_dir_all(&data)?;
    // 只读位置（光盘、受保护的文件夹）在这里就报出来，而不是启动到一半才失败
    let probe = data.join(".write-test");
    std::fs::write(&probe, b"")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}

/// 便携版的窗口都要经过这里：不指定数据目录，Tauri 会在 `%LOCALAPPDATA%` 下建一个
pub fn window<'a, R: Runtime, M: Manager<R>>(builder: WebviewWindowBuilder<'a, R, M>) -> WebviewWindowBuilder<'a, R, M> {
    match base() {
        Some(base) => builder.data_directory(data_dir(base).join("webview")),
        None => builder,
    }
}

/// 便携文件夹挪动过：把放在文件夹里的词典路径改到新位置（必须在加载词典之前调用）。
/// 文件夹外面的词典不动。
pub fn relocate(root: &Path, base: &Path) -> Result<usize> {
    let record = root.join(BASE_FILE);
    let current = base.to_string_lossy().into_owned();
    let previous = std::fs::read_to_string(&record).ok();
    let mut moved = 0;
    if let Some(old) = previous.as_deref().map(str::trim).filter(|s| !s.is_empty() && *s != current) {
        let db = Db::open(&root.join("lens.db"))?;
        moved = db.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut n = 0;
            for row in dicts::list(&tx)? {
                let path = row.path.as_deref().and_then(|p| rebase(p, Path::new(old), base));
                let index = row.index_path.as_deref().and_then(|p| rebase(p, Path::new(old), base));
                if path.is_some() || index.is_some() {
                    dicts::set_paths(&tx, &row.id, path.as_deref().or(row.path.as_deref()), index.as_deref().or(row.index_path.as_deref()))?;
                    n += 1;
                }
            }
            tx.commit()?;
            Ok(n)
        })?;
    }
    if previous.as_deref().map(str::trim) != Some(current.as_str()) {
        std::fs::write(&record, current)?;
    }
    Ok(moved)
}

/// `path` 在 `old` 里面时换成 `new` 下的同一位置，否则 None
fn rebase(path: &str, old: &Path, new: &Path) -> Option<String> {
    let rest = Path::new(path).strip_prefix(old).ok()?;
    Some(new.join(rest).to_string_lossy().into_owned())
}

/// 启动前的错误：这时还没有窗口和日志，弹系统对话框说明
#[cfg(windows)]
pub fn show_error(message: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    // SAFETY：两个字符串在调用期间都有效，没有父窗口
    unsafe {
        MessageBoxW(None, &HSTRING::from(message), &HSTRING::from("Lens"), MB_OK | MB_ICONERROR);
    }
}

#[cfg(not(windows))]
pub fn show_error(message: &str) {
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::dicts::{DictRow, IndexState};

    fn row(id: &str, path: &str) -> DictRow {
        DictRow {
            id: id.into(),
            name: id.into(),
            kind: "mdx".into(),
            path: Some(path.into()),
            category: None,
            entry_count: None,
            index_path: Some(format!("{path}.fst")),
            index_state: IndexState::Ready,
            created_at: 0,
            provider: None,
            options: None,
        }
    }

    fn paths(root: &Path) -> Result<Vec<(Option<String>, Option<String>)>> {
        let db = Db::open(&root.join("lens.db"))?;
        db.with_conn(|c| Ok(dicts::list(c)?.into_iter().map(|r| (r.path, r.index_path)).collect()))
    }

    #[test]
    fn moving_the_folder_updates_paths_inside_it_only() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let old = dir.path().join("E").join("Lens");
        let new = dir.path().join("F").join("Lens");
        let outside = dir.path().join("C").join("x.mdx").to_string_lossy().into_owned();
        let inside = old.join("dicts").join("oald.mdx").to_string_lossy().into_owned();
        // 名字以旧文件夹开头、但不在里面的路径不算
        let sibling = dir.path().join("E").join("Lens2").join("y.mdx").to_string_lossy().into_owned();
        {
            let db = Db::open(&dir.path().join("lens.db"))?;
            db.with_conn(|c| {
                dicts::insert(c, &row("a", &inside))?;
                dicts::insert(c, &row("b", &outside))?;
                dicts::insert(c, &row("c", &sibling))
            })?;
        }
        // 第一次运行只记位置，不改路径
        assert_eq!(relocate(dir.path(), &old)?, 0);
        assert_eq!(paths(dir.path())?[0].0.as_deref(), Some(inside.as_str()));
        // 换了位置
        assert_eq!(relocate(dir.path(), &new)?, 1);
        let got = paths(dir.path())?;
        let want = new.join("dicts").join("oald.mdx").to_string_lossy().into_owned();
        assert_eq!(got[0].0.as_deref(), Some(want.as_str()));
        assert_eq!(got[0].1, Some(format!("{want}.fst")));
        assert_eq!(got[1].0.as_deref(), Some(outside.as_str()));
        assert_eq!(got[2].0.as_deref(), Some(sibling.as_str()));
        // 再运行一次：位置没变，什么也不做
        assert_eq!(relocate(dir.path(), &new)?, 0);
        Ok(())
    }

    #[test]
    fn different_folders_get_different_instances() {
        let a = identifier(Path::new(r"E:\Lens"));
        assert_eq!(a, identifier(Path::new(r"E:\Lens")));
        assert_ne!(a, identifier(Path::new(r"F:\Lens")));
    }
}
