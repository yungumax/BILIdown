; 安装目录固定为 E:\02GJ\BILIdown（用户指定，不走 C 盘 LOCALAPPDATA）
; PREINSTALL 钩子在文件复制前运行，此时改 $INSTDIR 生效。
!macro NSIS_HOOK_PREINSTALL
  StrCpy $INSTDIR "E:\02GJ\BILIdown"
!macroend
