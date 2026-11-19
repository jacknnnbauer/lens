# 用本机 Calibre 把公版样本（《爱丽丝漫游奇境》前两章，Project Gutenberg #11）转成 mobi 测试文件。
# 生成物提交进仓库，测试不依赖 Calibre；改了 alice.html 才需要重跑。
$ErrorActionPreference = "Continue"
$dir = Join-Path $PSScriptRoot "..\src-tauri\tests\fixtures\mobi"
$conv = "C:\Program Files\Calibre2\ebook-convert.exe"
$src = Join-Path $dir "alice.html"
$common = @("--title", "Alice's Adventures in Wonderland", "--authors", "Lewis Carroll", "--language", "en",
  "--chapter", "//h:h2", "--level1-toc", "//h:h2", "--cover", (Join-Path $dir "alice.png"))
& $conv $src (Join-Path $dir "alice-old.mobi") @common --mobi-file-type old | Out-Null
& $conv $src (Join-Path $dir "alice-both.mobi") @common --mobi-file-type both | Out-Null
& $conv $src (Join-Path $dir "alice.azw3") @common | Out-Null
# 对照用：Calibre 自己转出的 EPUB（不提交）
& $conv $src (Join-Path $dir "alice-reference.epub") @common | Out-Null
Get-ChildItem $dir | Select-Object Name, Length
