!define COCUYO_BG "000000"
!define COCUYO_INK "F4F5F8"
!define COCUYO_DIM "8C8791"
!define COCUYO_FRAMES 16

!define MUI_BGCOLOR "${COCUYO_BG}"
!define MUI_TEXTCOLOR "${COCUYO_INK}"
!define MUI_INSTFILESPAGE_COLORS "${COCUYO_DIM} ${COCUYO_BG}"
!define MUI_INSTFILESPAGE_PROGRESSBAR "smooth"
!define MUI_CUSTOMFUNCTION_GUIINIT CocuyoGuiInit
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.CocuyoGuiInit

!define COCUYO_DIR "${__FILEDIR__}"
!define MUI_WELCOMEPAGE_TITLE "Cocuyo"
!define MUI_WELCOMEPAGE_TEXT "$(CocuyoWelcome)"
!define MUI_FINISHPAGE_TITLE "$(CocuyoDone)"
!define MUI_FINISHPAGE_TEXT "$(CocuyoDoneText)"

Var CocuyoImage ; El control que muestra la imagen lateral en la página actual.
Var CocuyoStep
Var CocuyoW
Var CocuyoH

!macro COCUYO_DARK un
  Function ${un}CocuyoDarken
    Push $1
    Push $2
    Push $3
    Push $4
    Push $5
    Push $6
    Push $7
    Push $8
    Push $9
    SetCtlColors $0 "${COCUYO_INK}" "${COCUYO_BG}"
    System::Call 'user32::GetWindow(p r0, i 5) p .r1' ; El primer hijo.
    ${DoWhile} $1 <> 0
      System::Call 'user32::GetClassNameW(p r1, w .r2, i 64)'
      ${If} $2 == "Button"
        System::Call 'user32::GetWindowLongW(p r1, i -16) i .r3'
        IntOp $3 $3 & 0xF
        ${If} $3 < 2
          System::Call 'uxtheme::SetWindowTheme(p r1, w "DarkMode_Explorer", p 0)'
        ${ElseIf} $3 <> 7
          SetCtlColors $1 "${COCUYO_INK}" "${COCUYO_BG}"
          System::Call 'user32::GetWindowTextW(p r1, w .r4, i 512)'
          System::Call '*(i, i, i, i) p .r5'
          System::Call 'user32::GetWindowRect(p r1, p r5)'
          System::Call 'user32::MapWindowPoints(p 0, p r0, p r5, i 2)'
          System::Call '*$5(i .r6, i .r7, i .r8, i .r9)' ; izquierda, arriba, derecha, abajo
          System::Free $5
          IntOp $9 $9 - $7 ; alto
          IntOp $8 $8 - $6 ; ancho
          IntOp $5 $9 + 2  ; lo que ocupa la caja
          System::Call 'user32::SetWindowPos(p r1, p 0, i 0, i 0, i r5, i r9, i 0x16)'
          IntOp $5 $5 + 4
          IntOp $6 $6 + $5
          IntOp $8 $8 - $5
          System::Call 'user32::CreateWindowExW(i 0, w "Static", w r4, i 0x50000200, i r6, i r7, i r8, i r9, p r0, p 0, p 0, p 0) p .r5'
          SendMessage $1 ${WM_GETFONT} 0 0 $6
          SendMessage $5 ${WM_SETFONT} $6 1
          SetCtlColors $5 "${COCUYO_INK}" "${COCUYO_BG}"
        ${EndIf}
      ${ElseIf} $2 == "Static"
        SetCtlColors $1 "${COCUYO_INK}" "${COCUYO_BG}"
      ${ElseIf} $2 == "Edit"
        SetCtlColors $1 "${COCUYO_INK}" "1B1B1D"
      ${ElseIf} $2 == "msctls_progress32"
        System::Call 'uxtheme::SetWindowTheme(p r1, w " ", w " ")'
        System::Call 'user32::GetWindowLongW(p r1, i -20) i .r3'
        IntOp $3 $3 & 0xFFFDFDFF ; Sin el borde hundido.
        System::Call 'user32::SetWindowLongW(p r1, i -20, i r3)'
        System::Call 'user32::SetWindowPos(p r1, p 0, i 0, i 0, i 0, i 0, i 0x37)'
        SendMessage $1 0x2001 0 0x001D1B1B
        SendMessage $1 0x0409 0 0x004BB8F2
      ${EndIf}
      System::Call 'user32::GetWindow(p r1, i 2) p .r1' ; El siguiente hermano.
    ${Loop}
    Pop $9
    Pop $8
    Pop $7
    Pop $6
    Pop $5
    Pop $4
    Pop $3
    Pop $2
    Pop $1
  FunctionEnd

  Function ${un}CocuyoTidy
    Push $0
    Push $1
    System::Call 'user32::GetWindow(p $HWNDPARENT, i 5) p .r0'
    ${DoWhile} $0 <> 0
      System::Call 'user32::GetClassNameW(p r0, w .r1, i 64)'
      ${If} $1 == "Static"
        System::Call 'user32::GetWindowLongW(p r0, i -16) i .r1'
        IntOp $1 $1 & 0x1F
        ${If} $1 = 0x10
          ShowWindow $0 ${SW_HIDE}
        ${EndIf}
      ${EndIf}
      System::Call 'user32::GetWindow(p r0, i 2) p .r0'
    ${Loop}
    GetDlgItem $0 $HWNDPARENT 1028
    ShowWindow $0 ${SW_HIDE}
    GetDlgItem $0 $HWNDPARENT 1256
    ShowWindow $0 ${SW_HIDE}
    Pop $1
    Pop $0
  FunctionEnd

  Function ${un}CocuyoGuiInit
    System::Call 'dwmapi::DwmSetWindowAttribute(p $HWNDPARENT, i 20, *i 1, i 4)'
    System::Call 'dwmapi::DwmSetWindowAttribute(p $HWNDPARENT, i 19, *i 1, i 4)'
    System::Call 'uxtheme::#135(i 2)'
    StrCpy $0 $HWNDPARENT
    Call ${un}CocuyoDarken
    Call ${un}CocuyoTidy
  FunctionEnd

  Function ${un}CocuyoFindPage
    Push $2
    StrCpy $0 0
    ${Do}
      FindWindow $0 "#32770" "" $HWNDPARENT $0
      ${If} $0 = 0
        ${ExitDo}
      ${EndIf}
      GetDlgItem $2 $0 $1
      ${If} $2 <> 0
        ${ExitDo}
      ${EndIf}
    ${Loop}
    Pop $2
  FunctionEnd

  Function ${un}CocuyoInstShow
    StrCpy $1 1004 ; La barra de progreso.
    Call ${un}CocuyoFindPage
    Call ${un}CocuyoDarken
    Call ${un}CocuyoTidy
    GetDlgItem $1 $0 1027 ; "Mostrar detalles"
    ShowWindow $1 ${SW_HIDE}
  FunctionEnd
!macroend

!insertmacro COCUYO_DARK ""
!insertmacro COCUYO_DARK "un."

Function CocuyoPulse
  Push $0
  Push $1
  Push $2
  IntOp $CocuyoStep $CocuyoStep + 1
  IntOp $0 $CocuyoStep % ${COCUYO_FRAMES}
  IntFmt $0 "%02d" $0
  System::Call 'user32::LoadImageW(p 0, w "$PLUGINSDIR\glow-$0.bmp", i 0, i $CocuyoW, i $CocuyoH, i 0x10) p .r1'
  ${If} $1 <> 0
    SendMessage $CocuyoImage ${STM_SETIMAGE} 0 $1 $2
    System::Call 'gdi32::DeleteObject(p r2)'
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

Function CocuyoStopPulse
  ${NSD_KillTimer} CocuyoPulse
FunctionEnd

Function CocuyoStartPulse
  StrCpy $1 1200
  Call CocuyoFindPage
  Call CocuyoDarken
  Call CocuyoTidy
  GetDlgItem $CocuyoImage $0 1200
  System::Call '*(i, i, i, i) p .r1'
  System::Call 'user32::GetClientRect(p $CocuyoImage, p r1)'
  System::Call '*$1(i, i, i .r2, i .r3)'
  System::Free $1
  StrCpy $CocuyoW $2
  StrCpy $CocuyoH $3
  ${NSD_CreateTimer} CocuyoPulse 110
FunctionEnd

Function CocuyoWelcomeShow
  InitPluginsDir
  SetOutPath $PLUGINSDIR
  File "${__FILEDIR__}\glow-*.bmp"
  Call CocuyoStartPulse
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:$(CocuyoInstall)"
FunctionEnd

Function CocuyoFinishShow
  Call CocuyoStartPulse
FunctionEnd
