// As páginas no navegador comum, com a ponte de mentira (sem o programa):
// miniatura (estados e aviso) e Main com o adaptador (árvore, conversa, criar), em 2040×790 e 1440×900.
// Uso: node ferramentas/testes/paginas.mjs
import { readFileSync, mkdirSync } from 'node:fs'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { servir } from '../servir-ui.mjs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const { chromium } = await import(pathToFileURL('A:/Claude/03-projetos/.ferramentas/node_modules/playwright/index.mjs').href)
const fotos = join(raiz, 'docs', 'capturas'); mkdirSync(fotos, { recursive: true })
const placar = []
const conferir = (nome, ok, det = '') => { placar.push(ok); console.log(`${ok ? 'OK  ' : 'FALHOU'} ${nome}${det ? ' — ' + det : ''}`) }
const PONTE = readFileSync(join(raiz, 'ui', 'ponte.js'), 'utf8')
const MAIN = readFileSync(join(raiz, 'ui', 'ponte-main.js'), 'utf8')

const srv = await servir(5197)
const nav = await chromium.launch()
try {
  // ----- miniatura -----
  const m = await nav.newPage({ viewport: { width: 400, height: 520 } })
  const erros = []; m.on('pageerror', (e) => erros.push(e.message))
  await m.goto('http://127.0.0.1:5197/mini.html')
  await m.waitForFunction(() => window.__mini)
  const estados = {}
  for (const k of ['esperando', 'trabalhando', 'precisa', 'erro']) {
    await m.evaluate((k) => window.ponte._simular('ponte:estado', { estado: k, legenda: 'Lendo LEIA-ME.md', aviso: null, mudo: false }), k)
    estados[k] = await m.evaluate(() => document.getElementById('linha-rotulo').textContent)
  }
  conferir('miniatura mostra cada estado na linha', estados.trabalhando === 'SARA · TRABALHANDO' && estados.precisa === 'SARA · PRECISA DE VOCÊ' && estados.erro === 'SARA · ERRO', JSON.stringify(estados))
  await m.evaluate(() => window.ponte._simular('ponte:estado', { estado: 'pronto', legenda: 'Trabalho pronto', aviso: { tipo: 'pronto', topo: 'SARA · PRONTO', titulo: 'Alex, seu trabalho está pronto.', texto: '<b>curso-mirian</b>: a landing', acao: 'VER AGORA', em: Date.now() }, som: true, mudo: false }))
  const aviso = await m.evaluate(() => ({ vis: !document.getElementById('aviso').hidden, texto: document.getElementById('aviso-texto').textContent, html: document.getElementById('aviso-texto').innerHTML, chama: document.getElementById('retrato').classList.contains('chama') }))
  conferir('aviso de pronto aparece, sem interpretar HTML do texto', aviso.vis && aviso.chama && aviso.texto.startsWith('<b>') && !/<b>/.test(aviso.html), aviso.texto)
  await m.screenshot({ path: join(fotos, 'mini-navegador.png') })
  await m.click('#aviso-depois')
  conferir('DEPOIS some com o aviso', await m.evaluate(() => document.getElementById('aviso').hidden))
  conferir('miniatura sem erro de script', erros.length === 0, erros.join(' | '))

  // ----- Main com o adaptador, nas larguras do Alex -----
  for (const [w, h] of [[2040, 790], [1440, 900]]) {
    const p = await nav.newPage({ viewport: { width: w, height: h } })
    const errosMain = []; p.on('pageerror', (e) => errosMain.push(e.message))
    await p.addInitScript(PONTE); await p.addInitScript(MAIN)
    await p.goto('http://127.0.0.1:5197/estudio/Main.dc.html')
    const ok = await p.waitForFunction(() => window.__ponteMain && window.__ponteMain.ok, null, { timeout: 20000 }).then(() => true, () => false)
    conferir(`${w}×${h}: adaptador ligado`, ok)
    const arvore = await p.evaluate(() => (document.querySelector('section[aria-label="Arquivos"]') || {}).innerText || '')
    conferir(`${w}×${h}: árvore vem da ponte`, /03-projetos/.test(arvore) && /LEIA-ME\.md/.test(arvore), arvore.replace(/\s+/g, ' ').slice(0, 80))
    await p.fill('#msg', 'teste de ida e volta')
    await p.press('#msg', 'Enter')
    const volta = await p.waitForFunction(() => /Resposta de teste \(fora do programa\)/.test(document.body.innerText) && !window.__ponteMain.logica().state.rodandoEm, null, { timeout: 8000 }).then(() => true, () => false)
    conferir(`${w}×${h}: Enter manda pela ponte e a resposta aparece uma vez`, volta && (await p.evaluate(() => document.body.innerText.split('Resposta de teste (fora do programa)').length - 1)) === 1)
    const estouro = await p.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1)
    conferir(`${w}×${h}: sem rolagem horizontal`, !estouro)
    await p.screenshot({ path: join(fotos, `main-navegador-${w}.png`) })
    conferir(`${w}×${h}: sem erro de script`, errosMain.length === 0, errosMain.join(' | ').slice(0, 200))
    await p.close()
  }
} finally {
  await nav.close(); srv.close()
}
const falhas = placar.filter((x) => !x).length
console.log(`\nplacar: ${placar.length - falhas}/${placar.length}`)
process.exit(falhas ? 1 : 0)
