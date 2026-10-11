; Hook NSIS: installazione opzionale di Equalizer APO (https://sourceforge.net/projects/equalizerapo/).
; NSIS hook: optional install of Equalizer APO.
;
; Referenziato da tauri.conf.json (bundle.windows.nsis.installerHooks).
;
; Cosa fa / What it does
;   - Equalizer APO NON e' incluso nell'installer di SwitchEQ (e' GPL v2+, programma
;     separato). Se manca, dopo la copia dei file l'installer CHIEDE (predefinito: No)
;     se scaricarlo da SourceForge.
;   - Versione e SHA-256 sono fissati qui sotto: un file diverso viene scartato e non
;     viene mai eseguito. Il download usa PowerShell (Invoke-WebRequest, TLS 1.2, segue
;     i redirect verso i mirror di SourceForge).
;   - Il setup di Equalizer APO NON e' firmato e richiede privilegi di amministratore
;     (UAC): l'utente viene avvisato prima. Al termine si apre il Device Selector
;     ufficiale, dove si scelgono i dispositivi. SwitchEQ non scrive il registro
;     MMDevices: la scelta resta a Equalizer APO.
;   - Salta tutto con installazione silenziosa (/S), passiva (/P) o aggiornamento
;     (/UPDATE), e se Equalizer APO e' gia' installato.
;
; Per disattivarlo: togliere "installerHooks" da tauri.conf.json.
; Per aggiornare la versione: cambiare i tre !define (versione, URL e SHA-256 del file
; scaricato; controllare l'hash con `Get-FileHash -Algorithm SHA256`).
;
; Il comportamento dell'installer di Equalizer APO e' dedotto dal suo sorgente; questo
; hook non e' stato eseguito end-to-end nei test automatici (vedi README).

!define SWITCHEQ_APO_VERSION "1.4.2"
!define SWITCHEQ_APO_URL "https://sourceforge.net/projects/equalizerapo/files/1.4.2/EqualizerAPO-x64-1.4.2.exe/download"
!define SWITCHEQ_APO_SHA256 "7403BE7427BBE1936A40DDED082829B6E217FC4F5990FEE5CBA501F0AE055AFA"
!define SWITCHEQ_APO_FILE "switcheq-EqualizerAPO-x64-${SWITCHEQ_APO_VERSION}.exe"

!macro NSIS_HOOK_POSTINSTALL
  ${IfNot} ${Silent}
  ${AndIf} $PassiveMode <> 1
  ${AndIf} $UpdateMode <> 1
    SetRegView 64
    ReadRegStr $0 HKLM "SOFTWARE\EqualizerAPO" "InstallPath"
    SetRegView 32
    ${If} $0 == ""
      ${If} $LANGUAGE = 1040
        MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Equalizer APO non risulta installato: SwitchEQ ne ha bisogno per applicare l'equalizzazione.$\n$\nVuoi scaricarlo e installarlo ora?$\n$\n- Viene scaricata la versione ${SWITCHEQ_APO_VERSION} da SourceForge (circa 12 MB) e ne viene verificata l'impronta SHA-256.$\n- Il programma di installazione di Equalizer APO NON e' firmato: SmartScreen o l'antivirus potrebbero avvisarti.$\n- Servono i privilegi di amministratore (UAC).$\n- Al termine si apre il Device Selector ufficiale di Equalizer APO, dove scegli le tue cuffie; potrebbe essere richiesto un riavvio.$\n$\nPuoi anche farlo piu' tardi da https://sourceforge.net/projects/equalizerapo/" IDNO switcheq_apo_done
      ${Else}
        MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Equalizer APO is not installed: SwitchEQ needs it to apply the equalization.$\n$\nDo you want to download and install it now?$\n$\n- Version ${SWITCHEQ_APO_VERSION} is downloaded from SourceForge (about 12 MB) and its SHA-256 hash is verified.$\n- The Equalizer APO installer is NOT code-signed: SmartScreen or your antivirus may warn you.$\n- Administrator rights (UAC) are required.$\n- When it finishes, the official Equalizer APO Device Selector opens so you can pick your headphones; a restart may be required.$\n$\nYou can also do this later from https://sourceforge.net/projects/equalizerapo/" IDNO switcheq_apo_done
      ${EndIf}

      DetailPrint "Equalizer APO ${SWITCHEQ_APO_VERSION}: download"
      Delete "$TEMP\${SWITCHEQ_APO_FILE}"
      nsExec::Exec 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$ErrorActionPreference = $\'Stop$\'; $$ProgressPreference = $\'SilentlyContinue$\'; [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; $$f = Join-Path $$env:TEMP $\'${SWITCHEQ_APO_FILE}$\'; Invoke-WebRequest -UseBasicParsing -UserAgent $\'SwitchEQ-installer$\' -Uri $\'${SWITCHEQ_APO_URL}$\' -OutFile $$f; if ((Get-FileHash -Algorithm SHA256 -LiteralPath $$f).Hash -ne $\'${SWITCHEQ_APO_SHA256}$\') { Remove-Item -LiteralPath $$f -Force; exit 2 }"'
      Pop $1
      ${If} $1 = 0
      ${AndIf} ${FileExists} "$TEMP\${SWITCHEQ_APO_FILE}"
        DetailPrint "Equalizer APO ${SWITCHEQ_APO_VERSION}: SHA-256 OK"
        ExecShellWait "runas" "$TEMP\${SWITCHEQ_APO_FILE}"
        Delete "$TEMP\${SWITCHEQ_APO_FILE}"
      ${Else}
        Delete "$TEMP\${SWITCHEQ_APO_FILE}"
        ${If} $LANGUAGE = 1040
          MessageBox MB_OK|MB_ICONEXCLAMATION "Download o verifica di Equalizer APO non riusciti (codice $1). Nessun file e' stato eseguito.$\n$\nInstallalo a mano da https://sourceforge.net/projects/equalizerapo/"
        ${Else}
          MessageBox MB_OK|MB_ICONEXCLAMATION "Downloading or verifying Equalizer APO failed (code $1). Nothing was run.$\n$\nInstall it manually from https://sourceforge.net/projects/equalizerapo/"
        ${EndIf}
      ${EndIf}
    ${EndIf}
    switcheq_apo_done:
  ${EndIf}
!macroend
