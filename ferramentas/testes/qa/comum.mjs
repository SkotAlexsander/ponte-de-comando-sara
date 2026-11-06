// Ajudante dos testes de QA (Tomás, relatório qa-01). Abre o .exe de verdade ISOLADO e FORA DA TELA:
// pasta de dados, perfil do WebView2, pasta de trabalho e porta próprios em %TEMP%\ponte-qa-<nome>.
// Nunca toca na config real do Alex nem no %APPDATA% do programa instalado.
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

export const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')
export const EXE = process.env.PONTE_QA_EXE || join(raiz, 'src-tauri', 'target', 'debug', 'ponte-de-comando.exe')
// caminhos ABSOLUTOS com a barra escapada (o limpar.mjs da casa tem \t e \v no lugar: ver o relatório)
export const PS = 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'
export const TASKKILL = 'C:\\Windows\\System32\\taskkill.exe'
export const esperar = (ms) => new Promise((r) => setTimeout(r, ms))
export async function ate(fn, ms = 15000, passo = 200) { const t0 = Date.now(); while (Date.now() - t0 < ms) { try { const v = await fn(); if (v) return v } catch {} await esperar(passo) } return null }

export const { chromium } = await import(pathToFileURL('A:/Claude/03-projetos/.ferramentas/node_modules/playwright/index.mjs').href)

export function ps(comando) {
  const r = spawnSync(PS, ['-NoProfile', '-NonInteractive', '-Command', comando], { encoding: 'utf8', windowsHide: true })
  return String(r.stdout || '').trim()
}

/** Encerra só os processos cuja linha de comando cita a pasta de teste. */
export function limpar(pasta) {
  const alvo = String(pasta).replace(/'/g, "''")
  const saida = ps(`Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains('${alvo}') -and $_.ProcessId -ne $PID } | ForEach-Object { $_.ProcessId }`)
  const pids = saida.split(/\r?\n/).map((x) => +x.trim()).filter((x) => x > 0)
  for (const pid of pids) spawnSync(TASKKILL, ['/PID', String(pid), '/T', '/F'], { windowsHide: true })
  return pids
}

export function filhos(pid) {
  const s = ps(`Get-CimInstance Win32_Process -Filter "ParentProcessId=${pid}" | ForEach-Object { "$($_.ProcessId) $($_.Name)" }`)
  return s.split(/\r?\n/).filter(Boolean).map((l) => { const [id, ...n] = l.split(' '); return { pid: +id, nome: n.join(' ') } })
}
export function descendentes(pid) {
  const todos = []
  const fila = [pid]
  while (fila.length) { const p = fila.shift(); for (const f of filhos(p)) { todos.push(f); fila.push(f.pid) } }
  return todos
}
export const vivo = (pid) => ps(`if (Get-Process -Id ${pid} -ErrorAction SilentlyContinue) { 'sim' } else { 'nao' }`) === 'sim'

/** CPU (s) e memória (MB) de um processo e de todos os descendentes. */
export function medirArvore(pid) {
  const ids = [pid, ...descendentes(pid).map((x) => x.pid)]
  const s = ps(`$t=0;$m=0; foreach($i in @(${ids.join(',')})){ $p=Get-Process -Id $i -ErrorAction SilentlyContinue; if($p){$t+=$p.TotalProcessorTime.TotalSeconds;$m+=$p.PrivateMemorySize64} }; "$t $m"`)
  const [cpu, mem] = s.split(' ').map(Number)
  return { cpu, memMB: mem / 1048576, processos: ids.length }
}

let portaBase = 47710, cdpBase = 9350
/**
 * Abre o programa isolado. opcoes: { config, envExtra, antes(base) , semEsperarMain }
 * Devolve { base, dados, trabalho, prog, nav, main, mini, pagina(re), info(), fechar() }
 */
export async function abrir(nome, opcoes = {}) {
  const base = join(tmpdir(), 'ponte-qa-' + nome)
  limpar(base)
  rmSync(base, { recursive: true, force: true })
  const dados = join(base, 'dados'), webview = join(base, 'webview'), trabalho = join(base, 'trabalho')
  for (const d of [dados, webview, join(trabalho, 'projeto-a')]) mkdirSync(d, { recursive: true })
  writeFileSync(join(trabalho, 'projeto-a', 'LEIA-ME.md'), '# projeto-a\n')
  const cfg = opcoes.config === undefined ? JSON.stringify({ pastaDeTrabalho: trabalho }) : opcoes.config
  if (cfg !== null) writeFileSync(join(dados, 'config.json'), typeof cfg === 'string' ? cfg : JSON.stringify(cfg))
  if (opcoes.antes) await opcoes.antes({ base, dados, trabalho })
  const porta = opcoes.porta || portaBase++, cdp = cdpBase++
  const env = { ...process.env, PONTE_PASTA_DE_DADOS: dados, WEBVIEW2_USER_DATA_FOLDER: webview, PONTE_PORTA: String(porta), PONTE_DEPURAR_PORTA: String(cdp), PONTE_TESTE_FORA_DA_TELA: '1', ...(opcoes.envExtra || {}) }
  delete env.CLAUDECODE
  const t0 = Date.now()
  const prog = spawn(EXE, [], { env, stdio: 'ignore', windowsHide: true })
  let saiu = null
  prog.on('exit', (c) => { saiu = c })
  const nav = await ate(() => chromium.connectOverCDP(`http://127.0.0.1:${cdp}`), 30000, 500)
  const paginas = () => (nav ? nav.contexts().flatMap((c) => c.pages()) : [])
  const pagina = (re) => paginas().find((p) => re.test(p.url()))
  const o = {
    base, dados, trabalho, porta, cdp, prog, nav, t0, env, pagina, paginas,
    get saiu() { return saiu },
    main: null, mini: null,
    info: () => o.main.evaluate(() => window.ponte.info()),
    async fechar() {
      if (nav) await nav.close().catch(() => {})
      if (saiu === null) spawnSync(TASKKILL, ['/PID', String(prog.pid), '/T', '/F'], { windowsHide: true })
      await esperar(500)
      limpar(base)
    },
  }
  if (!nav) return o
  if (!opcoes.semEsperarMain) {
    await ate(() => !pagina(/carregamento\.html/) && pagina(/Main\.dc\.html/), 40000, 250)
    o.main = pagina(/Main\.dc\.html/)
    o.mini = await ate(() => pagina(/mini\.html/), 10000)
  }
  return o
}

const placar = []
export function conferir(nome, ok, detalhe = '') {
  placar.push({ nome, ok: !!ok, detalhe })
  console.log(`${ok ? 'OK    ' : 'FALHOU'} ${nome}${detalhe ? ' — ' + detalhe : ''}`)
  return !!ok
}
export function fimDoPlacar() {
  const f = placar.filter((x) => !x.ok).length
  console.log(`\nplacar: ${placar.length - f}/${placar.length}`)
  return f
}
