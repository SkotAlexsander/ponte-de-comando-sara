// A regra da miniatura: some com a principal na tela, aparece quando ela é minimizada ou vai para a bandeja,
// e clicar nela traz a principal de volta. Mais: com a principal na tela o aviso vai para a Main, e nada de toast.
// Roda isolado e FORA DA TELA (PONTE_TESTE_FORA_DA_TELA=1): as janelas não aparecem para o Alex e não pegam o foco.
// Uso: node ferramentas/testes/miniatura.mjs [exe]
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { limparProcessos } from './limpar.mjs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const { chromium } = await import(pathToFileURL('A:/Claude/03-projetos/.ferramentas/node_modules/playwright/index.mjs').href)
const exe = process.argv.slice(2).find((a) => a.endsWith('.exe')) || join(raiz, 'src-tauri', 'target', 'debug', 'ponte-de-comando.exe')
const base = join(tmpdir(), 'ponte-teste-miniatura'); limparProcessos(base); rmSync(base, { recursive: true, force: true })
for (const d of ['dados', 'webview', 'trabalho']) mkdirSync(join(base, d), { recursive: true })
const dados = join(base, 'dados')
writeFileSync(join(dados, 'config.json'), JSON.stringify({ pastaDeTrabalho: join(base, 'trabalho') }))
const env = { ...process.env, PONTE_PASTA_DE_DADOS: dados, WEBVIEW2_USER_DATA_FOLDER: join(base, 'webview'), PONTE_PORTA: '47695', PONTE_DEPURAR_PORTA: '9335', PONTE_TESTE_FORA_DA_TELA: '1' }
delete env.CLAUDECODE
const esperar = (ms) => new Promise((r) => setTimeout(r, ms))
async function ate(fn, ms = 8000) { const t0 = Date.now(); while (Date.now() - t0 < ms) { try { const v = await fn(); if (v) return v } catch {} await esperar(150) } return null }
const placar = []
const conferir = (nome, ok, det = '') => { placar.push(ok); console.log(`${ok ? 'OK  ' : 'FALHOU'} ${nome}${det ? ' — ' + det : ''}`) }

const prog = spawn(exe, [], { env, stdio: 'ignore', windowsHide: true })
let nav
try {
  nav = await ate(() => chromium.connectOverCDP('http://127.0.0.1:9335'), 30000)
  const pag = (re) => nav.contexts().flatMap((c) => c.pages()).find((p) => re.test(p.url()))
  await ate(() => !pag(/carregamento/) && pag(/Main\.dc\.html/), 30000)
  const main = pag(/Main\.dc\.html/), mini = await ate(() => pag(/mini\.html/))
  const janelas = () => main.evaluate(() => window.ponte.info().then((i) => i.janelas))
  const esperaJanelas = (f) => ate(async () => { const j = await janelas(); return f(j) && j })

  let j = await esperaJanelas((j) => j.principal && j.mini === false)
  conferir('ao abrir: principal na tela, miniatura escondida', !!j, JSON.stringify(j))

  await main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize())
  j = await esperaJanelas((j) => j.principalMinimizada && j.mini)
  conferir('minimizar a principal: a miniatura aparece', !!j, JSON.stringify(j))

  // aviso com a principal minimizada: vai para a miniatura, sem toast (fora da instalação nunca há toast)
  spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { cwd: raiz, windowsHide: true, input: readFileSync(join(raiz, 'ferramentas', 'hook', 'exemplo-stop.json')), env: { ...process.env, PONTE_PASTA_DE_DADOS: dados } })
  const avisoMini = await ate(() => mini.evaluate(() => !document.getElementById('aviso').hidden && window.__ultimoSom))
  conferir('com a principal minimizada, o pronto aparece e toca na miniatura', !!avisoMini, JSON.stringify(avisoMini))
  await mini.evaluate(() => window.ponte.dispensarAviso())

  await mini.click('#retrato')
  j = await esperaJanelas((j) => j.principal && !j.principalMinimizada && j.mini === false)
  conferir('clicar na miniatura: a principal volta e a miniatura some', !!j, JSON.stringify(j))

  await main.evaluate(() => window.__TAURI__.window.getCurrentWindow().close())
  j = await esperaJanelas((j) => j.principal === false && j.mini)
  conferir('fechar a principal (vai para a bandeja): a miniatura aparece', !!j, JSON.stringify(j))

  await main.evaluate(() => window.ponte.mostrarPrincipal())
  j = await esperaJanelas((j) => j.principal && j.mini === false)
  conferir('restaurar a principal: a miniatura some', !!j, JSON.stringify(j))

  // aviso com a principal na tela: vai para a Main (placa + som), a miniatura continua escondida
  const somAntesMini = await mini.evaluate(() => window.__ultimoSom && window.__ultimoSom.quando)
  await esperar(10500) // o mesmo "pronto" em menos de 10 s não avisa duas vezes
  spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { cwd: raiz, windowsHide: true, input: readFileSync(join(raiz, 'ferramentas', 'hook', 'exemplo-stop.json')), env: { ...process.env, PONTE_PASTA_DE_DADOS: dados } })
  const naMain = await ate(() => main.evaluate(() => /SARA · PRONTO/.test(document.body.innerText) && window.__ultimoSomMain))
  const miniCalada = (await mini.evaluate(() => window.__ultimoSom && window.__ultimoSom.quando)) === somAntesMini
  j = await janelas()
  conferir('com a principal na tela, o pronto vai para a Main (placa e som) e a miniatura não reabre', !!naMain && miniCalada && j.mini === false, JSON.stringify(naMain))

  const log = readFileSync(join(dados, 'ponte.log'), 'utf8')
  conferir('nenhum toast fora da instalação (não sai como "Windows PowerShell")', !/notificação falhou/.test(log) && (await main.evaluate(() => window.ponte.info().then((i) => i.toast.instalado))) === false)

  // sem piscar: uma rajada de minimizar/restaurar termina no estado certo
  for (let i = 0; i < 4; i++) { await main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize()); await esperar(40); await main.evaluate(() => window.ponte.mostrarPrincipal()); await esperar(40) }
  j = await esperaJanelas((j) => j.principal && !j.principalMinimizada && j.mini === false)
  conferir('rajada de minimizar/restaurar termina com a miniatura escondida', !!j, JSON.stringify(j))

  // configurações: gravar na Main avisa a miniatura; chave ou valor errado é recusado sem gravar
  const cfg = await main.evaluate(() => window.ponte.config.gravar({ tema: 'fosforo', fosforo: 'verde', toastWindows: 'nunca' }))
  const temaMini = await ate(() => mini.evaluate(() => document.documentElement.dataset.tema === 'fosforo' && document.documentElement.dataset.fosforo === 'verde'), 3000)
  const arquivo = JSON.parse(readFileSync(join(dados, 'config.json'), 'utf8'))
  conferir('config.gravar grava e a miniatura segue o tema', cfg.tema === 'fosforo' && !!temaMini && arquivo.tema === 'fosforo' && arquivo.toastWindows === 'nunca', JSON.stringify(cfg))
  const ruins = await main.evaluate(async () => Promise.all([
    window.ponte.config.gravar({ tema: 'neon' }).then(() => 'gravou', (e) => e.message),
    window.ponte.config.gravar({ senha: 'x' }).then(() => 'gravou', (e) => e.message),
    window.ponte.config.gravar({ pastaTrabalho: 'Z:\\nao-existe' }).then(() => 'gravou', (e) => e.message),
  ]))
  const depois = await main.evaluate(() => window.ponte.config.ler())
  conferir('config recusa tema, chave e pasta inválidos', ruins.every((x) => x !== 'gravou') && depois.tema === 'fosforo', ruins.join(' / '))
  await main.evaluate(() => window.ponte.config.gravar({ miniAoMinimizar: false }))
  await main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize())
  await esperar(900)
  j = await janelas()
  conferir('"Miniatura ao minimizar" desligada: minimizar não mostra a miniatura', j.principalMinimizada && j.mini === false, JSON.stringify(j))
} catch (e) {
  conferir('o roteiro chegou ao fim', false, e.message)
} finally {
  if (nav) await nav.close().catch(() => {})
  spawnSync('C:\\Windows\\System32\\taskkill.exe', ['/PID', String(prog.pid), '/T', '/F'], { windowsHide: true })
  limparProcessos(base)
}
const falhas = placar.filter((x) => !x).length
console.log(`\nplacar: ${placar.length - falhas}/${placar.length}`)
process.exit(falhas ? 1 : 0)
