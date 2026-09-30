; 安装目录固定为 E:\02GJ\BILIdown（用户指定，不走 C 盘 LOCALAPPDATA）。
; Tauri 模板已定义 .onInit，再定义一个会报 "already exists"（CI 实测踩过）；
; 正确做法是用 NSIS 的 .onInit 追加机制：在宏里定义一个新的初始化函数，
; 再让模板的 .onInit……不行——模板不调用自定义函数。
; 真正可行的方式：Tauri 的 installerHooks 会在模板 .onInit 内部展开
; NSIS_HOOK_POSTINSTALL 等；但 .onInit 本身不可覆写。
; 替代：用 NSIS 内置的 InstallDir 命令通过 !system 在编译期……也不行。
; 最终方案：installerHooks 支持 NSIS_HOOK_POSTINSTALL 里做"移动"——
; 安装到默认目录后整体搬到 E:\02GJ\BILIdown 并更新注册表与卸载器。
!macro NSIS_HOOK_POSTINSTALL
  ; 停止刚启动的应用（NSIS passive 安装完会启动）
  nsExec::Exec 'taskkill /IM bilidown.exe /F'
  ; 整体搬迁
  CreateDirectory "E:\02GJ"
  CopyFiles /SILENT "$INSTDIR\*.*" "E:\02GJ\BILIdown"
  ; 删旧目录（连同卸载器，随后重写注册表指向新位置）
  RMDir /r "$INSTDIR"
  StrCpy $INSTDIR "E:\02GJ\BILIdown"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "DisplayIcon" "$INSTDIR\bilidown.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  ; 把卸载器复制到新位置并注册
  WriteUninstaller "$INSTDIR\uninstall.exe"
!macroend
