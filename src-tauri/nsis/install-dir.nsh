; 安装目录固定为 E:\02GJ\BILIdown（用户指定）。
; 机制：.onInit 里 RestorePreviousInstallLocation 会用注册表旧值覆盖 INSTDIR，
; 而 Section Install 开头已 SetOutPath $INSTDIR——PREINSTALL 再改 INSTDIR 太晚，
; 文件已写到旧目录。因此用 POSTINSTALL：安装完把整个目录搬到 E 盘，
; 并重写注册表三个键 + 在新位置重写卸载器。
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "迁移安装目录到 E:\02GJ\BILIdown ..."
  ; 结束可能被安装器拉起的应用
  nsExec::ExecToLog 'taskkill /IM bilidown.exe /F'
  Sleep 500
  CreateDirectory "E:\02GJ\BILIdown"
  ; ClearErrors 以便诊断
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\*" "E:\02GJ\BILIdown"
  IfErrors 0 +3
    DetailPrint "迁移复制失败"
    Goto skip_move
  Delete "$INSTDIR\*.*"
  RMDir "$INSTDIR"
  StrCpy $INSTDIR "E:\02GJ\BILIdown"
  WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "InstallLocation" "$INSTDIR"
  WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "DisplayIcon" "$INSTDIR\bilidown.exe"
  WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\BILIdown" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  DetailPrint "迁移完成"
  Goto move_done
skip_move:
move_done:
!macroend
