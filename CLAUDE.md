# CLAUDE.md · 词镜 Lens

桌面查词软件，Tauri 2（Rust）+ React + TypeScript，第一版只支持 Windows。完整规格见 `SPEC.md`，动手前先读相关章节。

## 工作约定

- 按 `SPEC.md` 第 2 节的里程碑顺序推进，一次只做一个里程碑里的一小块，做完跑测试再继续。
- 业务逻辑放 Rust（词典解析、索引、数据库、取词、导出调度）；前端只做展示和交互，通过 `src/ipc/` 里的类型化封装调用。
- Rust：不用 `unwrap()` / `expect()` 处理运行时数据；错误用 `thiserror` 定义，命令返回 `Result`。耗时操作放 `spawn_blocking` 或后台任务，不阻塞 IPC 线程。
- 前端颜色只用 `src/theme/tokens.css` 里的 CSS 变量，不写死颜色值。所有可点击元素用真实的 `<button>` / `<a>`，保证键盘可达。
- 词典 HTML 一律经过 DOMPurify 清洗后挂到 Shadow DOM，不执行词典里的脚本。
- 不提交任何商业词典文件；测试用 `fixtures/` 里自制的小词典。
- 新增数据库字段必须写迁移文件，不改已发布的迁移。

## 常用命令

```bash
pnpm install
pnpm tauri dev          # 开发运行
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test               # 前端单元测试（Vitest）
pnpm tauri build        # 打包（NSIS）
```

## 设计稿

https://claude.ai/code/artifact/0cf57411-c890-42c7-b38a-63a49b6a897f
画板编号：00 总览 · 01 主窗口 · 02 迷你弹窗 · 03 词典库 · 04 生词本 · 05 简洁单栏 · 06 设置 · 07/08 单词导出 · 09 主题库 · 10/11 批量导出 · 12 沉浸模式
