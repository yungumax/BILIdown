; 安装目录固定为 E:\02GJ\BILIdown（用户指定，不走 C 盘 LOCALAPPDATA）
; 用 .onInit：在安装器初始化阶段改 $INSTDIR，早于所有页面和文件复制。
; PREINSTALL 钩子实测太晚（文件已在默认目录解了一半才执行），
; 而 .onInit 是 NSIS 标准入口、总是最先运行。
Function .onInit
  StrCpy $INSTDIR "E:\02GJ\BILIdown"
FunctionEnd
