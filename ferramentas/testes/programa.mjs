// Teste de ponta a ponta do programa (o .exe de verdade), em ambiente ISOLADO:
// dados, perfil do WebView2, pasta de trabalho e porta próprios. Nunca toca nos dados do Alex.
// Uso: node ferramentas/testes/programa.mjs [caminho-do-exe] [--sem-claude]
//   --sem-claude  pula a conversa real (que gasta alguns centavos com o Haiku)
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { limparProcessos } from './limpar.mjs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const { chromium } = await import(pathToFileURL('A:/Claude/03-projetos/.ferramentas/node_modules/playwright/index.mjs').href)
const args = process.argv.slice(2)
const exe = args.find((a) => a.endsWith('.exe')) || join(raiz, 'src-tauri', 'target', 'debug', 'ponte-de-comando.exe')
const semClaude = args.includes('--sem-claude')
const PORTA = 47699, CDP = 9339

const base = join(tmpdir(), 'ponte-teste-programa')
limparProcessos(base)
rmSync(base, { recursive: true, force: true })
const dados = join(base, 'dados'), webview = join(base, 'webview'), trabalho = join(base, 'trabalho')
for (const d of [dados, webview, join(trabalho, 'projeto-a', 'design'), join(trabalho, 'node_modules')]) mkdirSync(d, { recursive: true })
writeFileSync(join(trabalho, 'projeto-a', 'LEIA-ME.md'), '# projeto-a\n\nlinha dois\n')
writeFileSync(join(dados, 'config.json'), JSON.stringify({ pastaDeTrabalho: trabalho, miniVisivel: true }))
const fotos = join(raiz, 'docs', 'capturas'); mkdirSync(fotos, { recursive: true })

const placar = []
const conferir = (nome, ok, detalhe = '') => { placar.push({ nome, ok }); console.log(`${ok ? 'OK  ' : 'FALHOU'} ${nome}${detalhe ? ' — ' + detalhe : ''}`) }
const esperar = (ms) => new Promise((r) => setTimeout(r, ms))
async function ate(fn, ms = 15000, passo = 200) { const t0 = Date.now(); while (Date.now() - t0 < ms) { try { const v = await fn(); if (v) return v } catch {} await esperar(passo) } return null }

const env = { ...process.env, PONTE_PASTA_DE_DADOS: dados, WEBVIEW2_USER_DATA_FOLDER: webview, PONTE_PORTA: String(PORTA), PONTE_DEPURAR_PORTA: String(CDP), PONTE_TESTE_FORA_DA_TELA: '1' }
delete env.CLAUDECODE
const t0 = Date.now()
const prog = spawn(exe, [], { env, stdio: 'ignore', windowsHide: true })
let saiu = null
prog.on('exit', (c) => { saiu = c })

let nav
try {
  nav = await ate(() => chromium.connectOverCDP(`http://127.0.0.1:${CDP}`), 30000, 500)
  conferir('o programa abriu e o WebView2 respondeu', !!nav, saiu !== null ? `saiu com código ${saiu}` : '')
  if (!nav) throw new Error('sem CDP')
  const paginas = () => nav.contexts().flatMap((c) => c.pages())
  const pagina = (re) => paginas().find((p) => re.test(p.url()))

  const carreg = await ate(() => pagina(/carregamento\.html/), 10000)
  conferir('janela de carregamento aberta primeiro', !!carreg)
  if (carreg) {
    const temArte = await ate(() => carreg.evaluate(() => !!document.querySelector('iframe') || !!document.querySelector('.provisorio')), 5000)
    const arte = await carreg.evaluate(() => document.querySelector('iframe') ? 'arte da Sara (estudio/carregando.html)' : 'provisório')
    conferir('carregamento mostra a arte', !!temArte, arte)
    const tam = await carreg.evaluate(() => [innerWidth, innerHeight])
    conferir('carregamento com 960×560', tam[0] === 960 && tam[1] === 560, tam.join('×'))
    await carreg.screenshot({ path: join(fotos, 'carregamento.png') }).catch(() => {})
  }
  const janelasCedo = carreg ? await carreg.evaluate(() => window.ponte.info().then((i) => i.janelas)).catch(() => null) : null
  // espera o carregamento sumir
  const fim = await ate(() => !pagina(/carregamento\.html/) && Date.now(), 30000, 250)
  const dur = ((fim || Date.now()) - t0) / 1000
  conferir('carregamento durou pelo menos 8 s e fechou', !!fim && dur >= 8, `${dur.toFixed(1)} s desde abrir o .exe`)
  conferir('principal e miniatura escondidas durante o carregamento', !!janelasCedo && janelasCedo.principal === false && janelasCedo.mini === false && janelasCedo.carregamento === true, JSON.stringify(janelasCedo))

  const main = await ate(() => pagina(/Main\.dc\.html/))
  const mini = await ate(() => pagina(/mini\.html/))
  const janelas = await ate(() => main.evaluate(() => window.ponte.info().then((i) => i.janelas)), 5000)
  conferir('janela principal abriu', !!janelas && janelas.principal === true)
  conferir('miniatura escondida com a principal na tela', !!janelas && janelas.mini === false)

  // ----- Main: adaptador, árvore real, pasta de trabalho -----
  const pm = await ate(() => main.evaluate(() => window.__ponteMain && window.__ponteMain.ok && { conversa: window.__ponteMain.assumirConversa, arvore: window.__ponteMain.arvoreReal }), 20000)
  conferir('adaptador da Main ligado (conversa + árvore real)', !!pm && pm.conversa && pm.arvore, JSON.stringify(pm))
  const arvore = await ate(() => main.evaluate(() => { const s = document.querySelector('section[aria-label="Arquivos"]'); const t = s ? s.innerText : ''; return /projeto-a/.test(t) && t }), 10000)
  conferir('Arquivos lista a pasta de trabalho real (sem node_modules)', !!arvore && !/node_modules/.test(arvore), (arvore || '').split('\n').slice(0, 4).join(' | '))
  const info = await main.evaluate(() => window.ponte.pastaDeTrabalho())
  conferir('pasta de trabalho gravada é a usada', info.caminho.toLowerCase() === trabalho.toLowerCase(), info.caminho)
  const criar = await main.evaluate(async (t) => {
    const r = {}
    r.pasta = await window.ponte.criarPasta(t + '\\projeto-a\\', 'nova-pasta').then((i) => i.caminho, (e) => 'ERRO ' + e.message)
    r.arquivo = await window.ponte.criarArquivo('projeto-a\\nova-pasta\\', 'notas.md').then((i) => i.caminho, (e) => 'ERRO ' + e.message)
    r.repetido = await window.ponte.criarArquivo('projeto-a\\', 'LEIA-ME.md').then(() => 'criou (errado)', (e) => e.message)
    r.fuga = await window.ponte.criarPasta('..\\', 'x').then(() => 'criou (errado)', (e) => e.message)
    r.fora = await window.ponte.criarPasta('C:\\Windows\\', 'x').then(() => 'criou (errado)', (e) => e.message)
    r.nome = await window.ponte.criarArquivo('', 'CON').then(() => 'criou (errado)', (e) => e.message)
    r.memoria = await window.ponte.criarArquivo('Biblioteca\\Memória\\', 'x.md').then(() => 'criou (errado)', (e) => e.message)
    r.ler = await window.ponte._invocar('ler_arquivo', { caminho: 'projeto-a\\LEIA-ME.md' })
    return r
  }, trabalho)
  conferir('cria pasta e arquivo dentro', criar.pasta === 'projeto-a\\nova-pasta' && criar.arquivo === 'projeto-a\\nova-pasta\\notas.md' && existsSync(join(trabalho, 'projeto-a', 'nova-pasta', 'notas.md')), `${criar.pasta} · ${criar.arquivo}`)
  conferir('recusa repetido, "..", caminho de fora, nome reservado e a Memória', [criar.repetido, criar.fuga, criar.fora, criar.nome, criar.memoria].every((x) => !/criou/.test(x)), [criar.repetido, criar.fuga, criar.fora, criar.nome, criar.memoria].join(' / '))
  conferir('LEIA-ME.md original intacto', readFileSync(join(trabalho, 'projeto-a', 'LEIA-ME.md'), 'utf8').startsWith('# projeto-a'))
  conferir('lê arquivo de texto (só leitura)', /linha dois/.test(criar.ler))

  // ----- FLUXO e volta -----
  await main.click('a[href="Fluxo.dc.html"]').catch(() => {})
  const fluxo = await ate(() => /Fluxo\.dc\.html/.test(main.url()), 8000)
  await esperar(800)
  await main.click('a[href="Main.dc.html"]').catch(() => {})
  const volta = await ate(() => /Main\.dc\.html/.test(main.url()), 8000)
  conferir('FLUXO abre o mapa e "← CONVERSA" volta', !!fluxo && !!volta)
  await ate(() => main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 15000)

  // ----- hook de pronto → miniatura (principal na bandeja) -----
  await main.evaluate(() => window.__TAURI__.window.getCurrentWindow().close())
  const naBandeja = await ate(() => main.evaluate(() => window.ponte.info().then((i) => !i.janelas.principal && i.janelas.mini)), 5000)
  conferir('fechar a principal: vai para a bandeja e a miniatura aparece', !!naBandeja)
  const antes = await mini.evaluate(() => document.getElementById('aviso').hidden)
  const r = spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs'), '--verboso'], { cwd: raiz, windowsHide: true, input: readFileSync(join(raiz, 'ferramentas', 'hook', 'exemplo-stop.json')), env: { ...process.env, PONTE_PASTA_DE_DADOS: dados } })
  conferir('hook saiu com 0', r.status === 0, String(r.stderr).trim())
  const aviso = await ate(() => mini.evaluate(() => !document.getElementById('aviso').hidden && { titulo: document.getElementById('aviso-titulo').textContent, texto: document.getElementById('aviso-texto').textContent, som: window.__ultimoSom }), 5000)
  conferir('miniatura mostra "Alex, seu trabalho está pronto." com o resumo', antes && aviso && aviso.titulo === 'Alex, seu trabalho está pronto.' && /landing do curso/.test(aviso.texto), aviso && `${aviso.titulo} · ${aviso.texto}`)
  conferir('som do aviso tocou (contexto de áudio rodando)', !!aviso && aviso.som && aviso.som.contexto === 'running', JSON.stringify(aviso && aviso.som))
  await mini.screenshot({ path: join(fotos, 'mini-pronto.png'), omitBackground: true }).catch(() => {})
  const placa = await ate(() => main.evaluate(() => /SARA · PRONTO/.test(document.body.innerText)), 4000)
  await mini.click('#retrato')
  const voltou = await ate(() => main.evaluate(() => window.ponte.info().then((i) => i.janelas.principal && !i.janelas.mini)), 5000)
  conferir('clicar na miniatura traz a principal e a miniatura some', !!voltou)
  conferir('placa da Main segue o evento (SARA · PRONTO)', !!placa)
  await mini.evaluate(() => window.ponte.dispensarAviso())
  const dispensou = await ate(() => mini.evaluate(() => document.getElementById('aviso').hidden), 3000)
  conferir('DEPOIS dispensa o aviso', !!dispensou)
  // token errado é recusado
  const ruim = await fetch(`http://127.0.0.1:${PORTA}/evento`, { method: 'POST', headers: { 'X-Ponte-Token': 'errado', 'Content-Type': 'application/json' }, body: '{"tipo":"pronto"}' }).then((x) => x.status).catch(() => 0)
  conferir('servidor recusa token errado', ruim === 401, String(ruim))
  // silenciar
  // QA B10: confere que o "mudo" foi gravado LIGADO e depois DESLIGADO (antes passava mesmo sem gravar nada)
  const mudoNoArquivo = () => { try { return JSON.parse(readFileSync(join(dados, 'config.json'), 'utf8')).mudo } catch { return undefined } }
  await mini.evaluate(() => window.ponte.definirMudo(true))
  const mudo = await ate(() => mini.evaluate(() => document.getElementById('bt-som').getAttribute('aria-pressed') === 'false'), 3000)
  const gravouLigado = await ate(() => mudoNoArquivo() === true, 3000)
  await mini.evaluate(() => window.ponte.definirMudo(false))
  const gravouDesligado = await ate(() => mudoNoArquivo() === false, 3000)
  conferir('silenciar chega na miniatura e é gravado', !!mudo && !!gravouLigado && !!gravouDesligado, `miniatura=${!!mudo} · gravou ligado=${!!gravouLigado} · gravou desligado=${!!gravouDesligado}`)

  // ----- conversa real pelo SDK, com cartão de permissão -----
  if (!semClaude) {
    const cfgs = await main.evaluate(async () => {
      const grupo = () => document.querySelector('[aria-label="Modelo e velocidade desta conversa"]')
      const bt = (n) => [...grupo().querySelectorAll('button')].find((b) => b.textContent.toUpperCase().includes(n))
      for (let i = 0; i < 3 && !/HAIKU/.test(grupo().textContent); i++) { bt('MODELO').click(); await new Promise((r) => setTimeout(r, 120)) }
      for (let i = 0; i < 4 && !/BAIXO/.test(grupo().textContent); i++) { bt('ESFORÇO').click(); await new Promise((r) => setTimeout(r, 120)) }
      return grupo().textContent.replace(/\s+/g, ' ')
    })
    await main.fill('#msg', 'Responda apenas com a palavra: VERDE')
    await main.press('#msg', 'Enter')
    const resposta = await ate(() => main.evaluate(() => /VERDE/.test([...document.querySelectorAll('*')].filter((n) => n.childElementCount === 0).map((n) => n.textContent).join(' ').replace('Responda apenas com a palavra: VERDE', '')) && !window.__ponteMain.logica().state.rodandoEm), 90000, 500)
    const conv = await main.evaluate(() => window.ponte.conversa((window.__ponteMain.logica().state.sessao || 'landing') + '@' + window.__ponteMain.carga))
    conferir('conversa real responde na Main (SDK, Haiku, esforço baixo)', !!resposta, `${cfgs} · sessão ${conv && conv.sessao} · ${conv && conv.modelo}/${conv && conv.esforco}`)
    conferir('modelo e esforço da aba chegaram ao backend', conv && conv.modelo === 'haiku' && conv.esforco === 'low' && conv.iniciada)
    await main.screenshot({ path: join(fotos, 'main-conversa.png') }).catch(() => {})

    // o padrão (modo guarda) liberaria este Write sozinho; o teste do cartão roda no modo 'sempre'
    await main.evaluate(() => window.ponte.config.gravar({ aprovacao: 'sempre' }))
    await main.fill('#msg', 'Use a ferramenta Write para criar o arquivo permissao-teste.txt com o texto oi. Não use nenhuma outra ferramenta.')
    await main.press('#msg', 'Enter')
    const cartao = await ate(() => main.evaluate(() => { const c = document.querySelector('[role=alertdialog]'); return c && c.style.display !== 'none' && c.innerText }), 90000, 500)
    conferir('cartão de permissão aparece em português', !!cartao && /PERMISSÃO/.test(cartao) && /Escrever o arquivo/.test(cartao), (cartao || '').split('\n').slice(0, 2).join(' · '))
    const precisa = await main.evaluate(() => window.ponte.estado())
    conferir('o estado vira "preciso de você" (aviso na Main, que está na tela)', precisa.estado === 'precisa' && precisa.onde === 'principal', precisa.aviso && precisa.aviso.titulo)
    await main.screenshot({ path: join(fotos, 'main-permissao.png') }).catch(() => {})
    if (cartao) await main.click('[role=alertdialog] [data-nao]')
    const terminou = await ate(() => main.evaluate(() => !window.__ponteMain.logica().state.rodandoEm), 90000, 500)
    conferir('negar não cria o arquivo e a conversa termina', !!terminou && !existsSync(join(trabalho, 'permissao-teste.txt')))
  }

  // ----- tokens e limites -----
  const tokens = (await ate(() => main.evaluate(() => window.ponte.tokens().then((t) => t.sessoes.length && t)), 120000, 2000)) || (await main.evaluate(() => window.ponte.tokens()))
  const medidas = tokens.sessoes.filter((x) => x.respostas)
  conferir('motor de tokens mede as sessões vivas do Claude Code', medidas.length > 0, `${tokens.sessoes.length} viva(s), ${medidas.length} medida(s), total estimado US$ ${tokens.total.custo.toFixed(2)}; ex.: ${medidas[0] ? medidas[0].nome + ' · contexto ' + medidas[0].contexto + ' · ' + medidas[0].agentes.length + ' agentes' : '-'}`)
  const lim = await ate(() => main.evaluate(() => window.ponte.limites().then((l) => l.disponivel && l)), 70000, 1000)
  conferir('limites de 5 h e 7 dias lidos do ~/.claude.json', !!lim, lim ? `5h ${lim.cincoHoras && lim.cincoHoras.porcento}% · 7d ${lim.seteDias && lim.seteDias.porcento}%` : '')
  await main.screenshot({ path: join(fotos, 'main.png') }).catch(() => {})
} catch (e) {
  conferir('o roteiro chegou ao fim', false, e.message)
} finally {
  if (nav) await nav.close().catch(() => {})
  // QA B10: sempre derruba a árvore do exe (mesmo que ele já tenha caído) e depois varre pela pasta de teste
  spawnSync('C:\\Windows\\System32\\taskkill.exe', ['/PID', String(prog.pid), '/T', '/F'], { windowsHide: true })
  await esperar(500)
  limparProcessos(base)
}
const falhas = placar.filter((x) => !x.ok).length
console.log(`\nplacar: ${placar.length - falhas}/${placar.length}`)
process.exit(falhas ? 1 : 0)
