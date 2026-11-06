// Depois de cada teste: encerra SÓ os processos que usam a pasta de teste (WebView2, node do sidecar, claude),
// achados pela linha de comando. Nunca mexe em outro programa do Alex.
import { spawnSync } from 'node:child_process'

// QA B9: a barra invertida precisa ser dobrada (com uma só, as sequências de tab e tab vertical estragavam o caminho e o spawn falhava calado)
const PS = 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'
const TASKKILL = 'C:\\Windows\\System32\\taskkill.exe'

export function limparProcessos(pastaDeTeste) {
  const alvo = String(pastaDeTeste).replace(/'/g, "''")
  const r = spawnSync(PS, ['-NoProfile', '-Command', `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${alvo}') -and $_.ProcessId -ne $PID } | ForEach-Object { $_.ProcessId }`], { encoding: 'utf8', windowsHide: true })
  if (r.error) throw new Error('limparProcessos: não consegui rodar o PowerShell: ' + r.error.message)
  const pids = String(r.stdout || '').split(/\r?\n/).map((x) => +x.trim()).filter((x) => x > 0)
  for (const pid of pids) spawnSync(TASKKILL, ['/PID', String(pid), '/T', '/F'], { windowsHide: true })
  return pids
}
