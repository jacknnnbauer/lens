//! 批量导出用到的系统接口：磁盘剩余空间、进程树内存（含 WebView2 子进程）、导出期间防睡眠。

use std::path::Path;

/// 目录所在磁盘的剩余可用字节数；拿不到时返回 None。
#[cfg(windows)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    // 目录可能还没创建：往上找到存在的那一级
    let mut probe = dir.to_path_buf();
    while !probe.exists() {
        probe = probe.parent()?.to_path_buf();
    }
    let mut free = 0u64;
    // SAFETY: 只写入本地变量 free。
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(probe.as_os_str()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}

#[cfg(not(windows))]
pub fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// 本进程及其所有子孙进程（WebView2 的浏览器、渲染、GPU 进程）的工作集之和（MB）。
#[cfg(windows)]
pub fn process_tree_mb() -> u64 {
    use std::collections::HashMap;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: 快照句柄与进程句柄用完即关；结构体按文档设置 dwSize / cb。
    unsafe {
        let me = GetCurrentProcessId();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snap, &mut e).is_ok() {
                loop {
                    children.entry(e.th32ParentProcessID).or_default().push(e.th32ProcessID);
                    if Process32NextW(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        let mut stack = vec![me];
        let mut seen = std::collections::HashSet::new();
        let mut total: u64 = 0;
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(c) = children.get(&pid) {
                stack.extend(c.iter().copied().filter(|&p| p != pid));
            }
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { continue };
            let mut pmc = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
            if GetProcessMemoryInfo(h, &mut pmc, pmc.cb).is_ok() {
                total += pmc.WorkingSetSize as u64;
            }
            let _ = CloseHandle(h);
        }
        total / (1024 * 1024)
    }
}

#[cfg(not(windows))]
pub fn process_tree_mb() -> u64 {
    0
}

/// 导出期间阻止系统睡眠（SPEC 12.3）。`SetThreadExecutionState` 按线程生效，
/// 所以开一个专用线程持有该状态，守卫 drop 时线程恢复设置并退出。
pub struct KeepAwake {
    stop: Option<std::sync::mpsc::Sender<()>>,
}

impl KeepAwake {
    pub fn start() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new().name("lens-keep-awake".into()).spawn(move || {
            #[cfg(windows)]
            {
                use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
                // SAFETY: 只影响本线程的执行状态。
                unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
                let _ = rx.recv();
                unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
            }
            #[cfg(not(windows))]
            let _ = rx.recv();
        });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "无法启动防睡眠线程");
        }
        Self { stop: Some(tx) }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_something() {
        assert!(free_bytes(&std::env::temp_dir().join("lens-no-such-dir/x")).is_some_and(|b| b > 0));
        assert!(process_tree_mb() > 0);
        drop(KeepAwake::start());
    }
}
