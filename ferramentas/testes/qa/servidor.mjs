// QA · servidor dos hooks: JSON quebrado, corpo gigante, token errado, conexões lentas e porta ocupada.
// Cada "OK" é o comportamento ESPERADO; cada "FALHOU" é um bug provado.
// Uso: node ferramentas/testes/qa/servidor.mjs
import net from 'node:net'
import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate, raiz, ps } from './comum.mjs'

/** Pedido HTTP cru; devolve { status, erro, ms }. */
function cru(porta, bruto, { fecharEscrita = false, timeout = 6000 } = {}) {
  return new Promise((ok) => {
    const t0 = Date.now()
    const s = net.connect(porta, '127.0.0.1')
    let r = ''
    s.setTimeout(timeout, () => { s.destroy(); ok({ status: 0, erro: 'timeout', ms: Date.now() - t0 }) })
    s.on('connect', () => { s.write(bruto); if (fecharEscrita) s.end() })
    s.on('data', (d) => { r += d })
    s.on('error', (e) => ok({ status: +(r.match(/^HTTP\/1\.1 (\d+)/) || [0, 0])[1], erro: e.code, ms: Date.now() - t0 }))
    s.on('close', () => ok({ status: +(r.match(/^HTTP\/1\.1 (\d+)/) || [0, 0])[1], erro: null, ms: Date.now() - t0 }))
  })
}

const app = await abrir('servidor')
try {
  const { porta, dados } = app
  const token = readFileSync(join(dados, 'token'), 'utf8').trim()
  const post = (corpo, extra = '') => `POST /evento HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Ponte-Token: ${token}\r\nContent-Type: application/json\r\n${extra}Content-Length: ${Buffer.byteLength(corpo)}\r\n\r\n${corpo}`

  // ----- só 127.0.0.1 -----
  const escuta = ps(`Get-NetTCPConnection -State Listen -LocalPort ${porta} -ErrorAction SilentlyContinue | ForEach-Object { $_.LocalAddress }`)
  conferir('o servidor escuta só em 127.0.0.1', escuta === '127.0.0.1', escuta)

  // ----- JSON quebrado e tipos errados -----
  const quebrado = await cru(porta, post('{"tipo":"pronto",'))
  conferir('JSON quebrado → 400', quebrado.status === 400, JSON.stringify(quebrado))
  const tipoErrado = await cru(porta, post('{"tipo":123}'))
  conferir('campo com tipo errado → 400 (sem derrubar nada)', tipoErrado.status === 400, JSON.stringify(tipoErrado))
  const semCorpo = await cru(porta, `POST /evento HTTP/1.1\r\nX-Ponte-Token: ${token}\r\n\r\n`)
  conferir('POST sem Content-Length → 400', semCorpo.status === 400, JSON.stringify(semCorpo))

  // ----- corpo gigante -----
  const gigante = await cru(porta, post(JSON.stringify({ tipo: 'pronto', texto: 'x'.repeat(200 * 1024) })))
  conferir('corpo de 200 KB → 413 recebido pelo cliente', gigante.status === 413, JSON.stringify(gigante))
  const cabecalho = await cru(porta, `POST /evento HTTP/1.1\r\nX-Lixo: ${'a'.repeat(10000)}\r\n\r\n`)
  conferir('cabeçalho de 10 KB → 431 recebido pelo cliente', cabecalho.status === 431, JSON.stringify(cabecalho))

  // ----- token errado: o cliente precisa RECEBER o 401 (não um reset) -----
  let resets = 0, recebidos401 = 0
  const corpo = JSON.stringify({ tipo: 'esperando', texto: 'y'.repeat(4000) })
  for (let i = 0; i < 40; i++) {
    const r = await cru(porta, `POST /evento HTTP/1.1\r\nX-Ponte-Token: errado\r\nContent-Length: ${corpo.length}\r\n\r\n${corpo}`)
    if (r.status === 401) recebidos401++
    else if (r.erro === 'ECONNRESET') resets++
  }
  conferir('token errado com corpo: o cliente recebe 401 em 40 de 40', recebidos401 === 40, `401 recebido ${recebidos401}/40, conexão resetada ${resets}/40`)
  let resetsOrigin = 0
  for (let i = 0; i < 20; i++) {
    const r = await cru(porta, `POST /evento HTTP/1.1\r\nOrigin: http://mal.com\r\nX-Ponte-Token: ${token}\r\nContent-Length: ${corpo.length}\r\n\r\n${corpo}`)
    if (r.status !== 403) resetsOrigin++
  }
  conferir('Origin com corpo: o cliente recebe 403 em 20 de 20', resetsOrigin === 0, `sem 403: ${resetsOrigin}/20`)

  // cabeçalho e corpo em pacotes separados (o que acontece com a máquina ocupada): o servidor responde
  // 401 sem ler o corpo e fecha; o corpo chega num soquete fechado e o Windows manda RST
  let semResposta = 0
  for (let i = 0; i < 30; i++) {
    const r = await new Promise((ok) => {
      const s = net.connect(porta, '127.0.0.1'); let txt = ''
      s.on('connect', () => { s.write(`POST /evento HTTP/1.1
X-Ponte-Token: errado
Content-Length: ${corpo.length}

`); setTimeout(() => { try { s.write(corpo) } catch {} }, 30); setTimeout(() => { try { s.write(corpo) } catch {} }, 60) })
      s.on('data', (d) => { txt += d })
      s.on('error', (e) => ok({ status: +(txt.match(/^HTTP\/1\.1 (\d+)/) || [0, 0])[1], erro: e.code }))
      s.on('close', () => ok({ status: +(txt.match(/^HTTP\/1\.1 (\d+)/) || [0, 0])[1], erro: null }))
    })
    if (r.status !== 401) semResposta++
  }
  conferir('token errado com o corpo chegando depois do cabeçalho: o cliente recebe 401 em 30 de 30', semResposta === 0, `sem o 401 (conexão resetada antes de ler): ${semResposta}/30`)

  // ----- conexões lentas não podem calar o hook -----
  const lentas = []
  for (let i = 0; i < 16; i++) {
    const s = net.connect(porta, '127.0.0.1')
    s.on('error', () => {})
    s.on('connect', () => s.write('POST /evento HTTP/1.1\r\n'))
    lentas.push(s)
  }
  // um byte a cada 2 s: cada leitura chega antes do limite de 3 s
  const pinga = setInterval(() => lentas.forEach((s) => { try { s.write('X') } catch {} }), 2000)
  await esperar(4500)
  const legitimo = await cru(porta, post(JSON.stringify({ tipo: 'trabalhando', fase: 'pedido', titulo: 'qa lento' })), { timeout: 4000 })
  conferir('com 16 conexões lentas abertas, o hook legítimo ainda é atendido', legitimo.status === 200, JSON.stringify(legitimo))
  clearInterval(pinga); lentas.forEach((s) => s.destroy())
  await esperar(3500)

  // ----- o hook (script) com stdin quebrado ou gigante sai com 0 e rápido -----
  const hook = join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')
  const envHook = { ...process.env, PONTE_PASTA_DE_DADOS: dados, PONTE_PORTA: String(porta) }
  let t0 = Date.now()
  const h1 = spawnSync(process.execPath, [hook], { input: '{"hook_event_name":"Stop",', env: envHook, windowsHide: true })
  conferir('hook com stdin quebrado sai com 0, calado', h1.status === 0 && String(h1.stdout) === '', `código ${h1.status} em ${Date.now() - t0} ms`)
  t0 = Date.now()
  const h2 = spawnSync(process.execPath, [hook], { input: JSON.stringify({ hook_event_name: 'UserPromptSubmit', prompt: 'p'.repeat(5_000_000), cwd: 'C:\\x' }), env: envHook, windowsHide: true })
  conferir('hook com stdin de 5 MB sai com 0 em menos de 3 s', h2.status === 0 && Date.now() - t0 < 3000, `código ${h2.status} em ${Date.now() - t0} ms`)
  const vivo = await app.main.evaluate(() => window.ponte.info().then((i) => !!i.porta))
  conferir('o programa continua de pé depois de tudo', vivo && app.saiu === null)
} catch (e) {
  conferir('o roteiro chegou ao fim', false, e.stack)
} finally {
  await app.fechar()
}

// ----- porta ocupada -----
// um "outro programa" escuta na porta; o arquivo `porta` da abertura anterior ficou lá
const capturados = []
const intruso = net.createServer((s) => { s.on('error', () => {}); s.on('data', (d) => { capturados.push(String(d)); s.end('HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok') }) })
const PORTA_OCUPADA = 47790
await new Promise((ok) => intruso.listen(PORTA_OCUPADA, '127.0.0.1', ok))
const app2 = await abrir('porta-ocupada', { porta: PORTA_OCUPADA, antes: ({ dados }) => writeFileSync(join(dados, 'porta'), String(PORTA_OCUPADA)) })
try {
  const info = await app2.info()
  const log = readFileSync(join(app2.dados, 'ponte.log'), 'utf8')
  conferir('porta ocupada: o registro diz que o servidor não abriu', /NÃO abriu/.test(log), (log.match(/servidor dos hooks[^\n]*/) || [''])[0])
  const estado = await app2.main.evaluate(() => window.ponte.estado())
  const avisa = /hook|servidor|porta \d+/i.test(JSON.stringify(estado)) || (await app2.main.evaluate(() => /hooks?|servidor dos|porta \d{4,5}/i.test(document.body.innerText)))
  conferir('porta ocupada: a tela ou a Sara avisa que os hooks não vão chegar', avisa, `info.porta=${info.porta}; estado=${estado.estado}/${estado.legenda}`)
  conferir('porta ocupada: o programa tenta outra porta (ou apaga o arquivo `porta` velho)', info.porta !== null || !existsSync(join(app2.dados, 'porta')), `info.porta=${info.porta}; arquivo porta=${existsSync(join(app2.dados, 'porta')) ? readFileSync(join(app2.dados, 'porta'), 'utf8') : '(nenhum)'}`)
  // o hook lê o arquivo velho e manda o token para quem estiver na porta
  spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { input: readFileSync(join(raiz, 'ferramentas', 'hook', 'exemplo-prompt.json')), env: { ...process.env, PONTE_PASTA_DE_DADOS: app2.dados }, windowsHide: true })
  await esperar(500)
  const token = readFileSync(join(app2.dados, 'token'), 'utf8').trim()
  const vazou = capturados.some((c) => c.includes(token))
  conferir('porta ocupada: o token do hook NÃO vai para o outro programa', !vazou, vazou ? 'o intruso recebeu X-Ponte-Token com o token verdadeiro' : '')
} catch (e) {
  conferir('o roteiro da porta ocupada chegou ao fim', false, e.stack)
} finally {
  await app2.fechar()
  intruso.close()
}
process.exit(fimDoPlacar() ? 1 : 0)
