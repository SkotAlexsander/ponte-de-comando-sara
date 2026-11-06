// Depois de cada teste: encerra SÓ os processos que usam a pasta de teste (WebView2, node do sidecar, claude),
// achados pela linha de comando. Nunca mexe em outro programa do Alex.
import { spawnSync } from 'node:child_process'

const PS = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
const TASKKILL = 'C:\Windows\System32\taskkill.exe'

export function limparProcessos(pastaDeTeste) {
  const alvo = String(pastaDeTeste).replace(/'/g, "''")
  const r = spawnSync(PS, ['-NoProfile', '-Command', `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${alvo}') -and $_.ProcessId -ne $PID } | ForEach-Object { $_.ProcessId }`], { encoding: 'utf8', windowsHide: true })
  const pids = String(r.stdout || '').split(/\r?\n/).map((x) => +x.trim()).filter((x) => x > 0)
  for (const pid of pids) spawnSync(TASKKILL, ['/PID', String(pid), '/T', '/F'], { windowsHide: true })
  return pids
}
