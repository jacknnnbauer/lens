; 资源管理器右键菜单「用 Lens 打开」（.md / .markdown / .txt / .epub）。
; 写在 SystemFileAssociations 下，只加一个菜单项，不改这些文件的默认打开程序。
; 文件须保存为带 BOM 的 UTF-8，NSIS 才按 Unicode 读中文。

!macro LENS_OPEN_WITH EXT
  WriteRegStr SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\OpenWithLens" "" "用 Lens 打开"
  WriteRegStr SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\OpenWithLens" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\OpenWithLens\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro LENS_REMOVE_OPEN_WITH EXT
  DeleteRegKey SHCTX "Software\Classes\SystemFileAssociations\.${EXT}\shell\OpenWithLens"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro LENS_OPEN_WITH "md"
  !insertmacro LENS_OPEN_WITH "markdown"
  !insertmacro LENS_OPEN_WITH "txt"
  !insertmacro LENS_OPEN_WITH "epub"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro LENS_REMOVE_OPEN_WITH "md"
  !insertmacro LENS_REMOVE_OPEN_WITH "markdown"
  !insertmacro LENS_REMOVE_OPEN_WITH "txt"
  !insertmacro LENS_REMOVE_OPEN_WITH "epub"
!macroend
