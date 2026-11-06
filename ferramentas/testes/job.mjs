// Fechar a Ponte (mesmo à força) encerra o sidecar e o claude que ele abriu (Job Object do Windows).
// Uso: node ferramentas/testes/job.mjs   (gasta uma chamada curta do Haiku)
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { limparProcessos } from './limpar.mjs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const { chromium } = await import(pathToFileURL('A:/Claude/03-projetos/.ferramentas/node_modules/playwright/index.mjs').href)
const exe = process.argv[2] || join(raiz, 'src-tauri', 'target', 'debug', 'ponte-de-comando.exe')
const base = join(tmpdir(), 'ponte-teste-job'); limparProcessos(base); rmSync(base, { recursive: true, force: true })
for (const d of ['dados', 'webview', 'trabalho']) mkdirSync(join(base, d), { recursive: true })
writeFileSync(join(base, 'dados', 'config.json'), JSON.stringify({ pastaDeTrabalho: join(base, 'trabalho') }))
const env = { ...process.env, PONTE_PASTA_DE_DADOS: join(base, 'dados'), WEBVIEW2_USER_DATA_FOLDER: join(base, 'webview'), PONTE_PORTA: '47696', PONTE_DEPURAR_PORTA: '9336', PONTE_TESTE_FORA_DA_TELA: '1' }
delete env.CLAUDECODE
const esperar = (ms) => new Promise((r) => setTimeout(r, ms))

// filhos de um PID (WMI pelo PowerShell do System32, caminho absoluto)
const PS = 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'
const filhos = (pid) => {
  const r = spawnSync(PS, ['-NoProfile', '-Command', `Get-CimInstance Win32_Process -Filter "ParentProcessId=${pid}" | ForEach-Object { "$($_.ProcessId) $($_.Name)" }`], { encoding: 'utf8', windowsHide: true })
  return r.stdout.trim().split(/\r?\n/).filter(Boolean).map((l) => { const [id, ...n] = l.split(' '); return { pid: +id, nome: n.join(' ') } })
}
const vivo = (pid) => spawnSync(PS, ['-NoProfile', '-Command', `if (Get-Process -Id ${pid} -ErrorAction SilentlyContinue) { 'sim' } else { 'nao' }`], { encoding: 'utf8', windowsHide: true }).stdout.trim() === 'sim'

const prog = spawn(exe, [], { env, stdio: 'ignore', windowsHide: true })
let ok = false
try {
  let nav = null
  for (let i = 0; i < 40 && !nav; i++) { nav = await chromium.connectOverCDP('http://127.0.0.1:9336').catch(() => null); if (!nav) await esperar(500) }
  let main = null
  for (let i = 0; i < 60 && !main; i++) { main = nav.contexts().flatMap((c) => c.pages()).find((p) => /Main\.dc\.html/.test(p.url())); if (!main) await esperar(500) }
  await main.evaluate(() => window.ponte.enviar({ conversa: 'job@1', texto: 'Conte de 1 a 200, um número por linha.', modelo: 'haiku', esforco: 'low' }))
  let node = null, claude = null
  for (let i = 0; i < 60 && !(node && claude); i++) {
    await esperar(500)
    node = filhos(prog.pid).find((f) => /node/i.test(f.nome))
    if (node) claude = filhos(node.pid).find((f) => /claude/i.test(f.nome))
  }
  console.log('sidecar:', node, '· claude:', claude)
  await nav.close().catch(() => {})
  spawnSync('C:\\Windows\\System32\\taskkill.exe', ['/PID', String(prog.pid), '/F'], { windowsHide: true }) // só a Ponte, sem /T
  await esperar(2500)
  const sobrou = [node, claude].filter((x) => x && vivo(x.pid))
  ok = !!node && !!claude && sobrou.length === 0
  console.log(ok ? 'OK   fechar a Ponte à força encerrou o sidecar e o claude' : `FALHOU sobrou: ${JSON.stringify(sobrou)}`)
} catch (e) {
  console.log('FALHOU', e.message)
  spawnSync('C:\\Windows\\System32\\taskkill.exe', ['/PID', String(prog.pid), '/T', '/F'], { windowsHide: true })
}
limparProcessos(base)
process.exit(ok ? 0 : 1)
