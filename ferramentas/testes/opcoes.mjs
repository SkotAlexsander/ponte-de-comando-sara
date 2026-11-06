// Configurações "inteligentes": chaves novas, cofre (credencial DE TESTE, apagada no fim), conta, idioma das
// respostas (Haiku de verdade, centavos), exportar/importar, restaurar e começar na bandeja.
// Roda isolado e fora da tela. Nunca usa nem toca a credencial do Alex; nenhuma chave real entra aqui.
// Uso: node ferramentas/testes/opcoes.mjs [exe] [--sem-claude]
import { spawn, spawnSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { randomBytes } from 'node:crypto'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { limparProcessos } from './limpar.mjs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const { chromium } = await import(pathToFileURL('A:/Claude/03-projetos/.ferramentas/node_modules/playwright/index.mjs').href)
const args = process.argv.slice(2)
const exe = args.find((a) => a.endsWith('.exe')) || join(raiz, 'src-tauri', 'target', 'debug', 'ponte-de-comando.exe')
const semClaude = args.includes('--sem-claude')
const base = join(tmpdir(), 'ponte-teste-opcoes'); limparProcessos(base); rmSync(base, { recursive: true, force: true })
for (const d of ['dados', 'webview', 'trabalho']) mkdirSync(join(base, d), { recursive: true })
const dados = join(base, 'dados')
const ALVO = 'PonteDeComando-teste-e2e-' + randomBytes(3).toString('hex')
// chave FALSA, no formato certo, só para o teste (a API responde 401)
const CHAVE_FALSA = 'sk-ant-teste-' + 'x'.repeat(30)
const CMDKEY = 'C:\\Windows\\System32\\cmdkey.exe'
const esperar = (ms) => new Promise((r) => setTimeout(r, ms))
async function ate(fn, ms = 10000) { const t0 = Date.now(); while (Date.now() - t0 < ms) { try { const v = await fn(); if (v) return v } catch {} await esperar(200) } return null }
const placar = []
const conferir = (nome, ok, det = '') => { placar.push(ok); console.log(`${ok ? 'OK  ' : 'FALHOU'} ${nome}${det ? ' — ' + det : ''}`) }

function abrir(cfg, cdp) {
  writeFileSync(join(dados, 'config.json'), JSON.stringify({ pastaDeTrabalho: join(base, 'trabalho'), ...cfg }))
  const env = { ...process.env, PONTE_PASTA_DE_DADOS: dados, WEBVIEW2_USER_DATA_FOLDER: join(base, 'webview'), PONTE_PORTA: '47694', PONTE_DEPURAR_PORTA: String(cdp), PONTE_TESTE_FORA_DA_TELA: '1', PONTE_AUTOSTART_FALSO: '1', PONTE_COFRE_ALVO: ALVO }
  delete env.CLAUDECODE
  return spawn(exe, [], { env, stdio: 'ignore', windowsHide: true })
}
const fechar = (prog) => spawnSync('C:\\Windows\\System32\\taskkill.exe', ['/PID', String(prog.pid), '/T', '/F'], { windowsHide: true })

let prog = abrir({}, 9334), nav
try {
  nav = await ate(() => chromium.connectOverCDP('http://127.0.0.1:9334'), 30000)
  const pag = (re) => nav.contexts().flatMap((c) => c.pages()).find((p) => re.test(p.url()))
  const main = await ate(() => !pag(/carregamento/) && pag(/Main\.dc\.html/), 30000)
  await ate(() => main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 20000)
  const ev = (fn, arg) => main.evaluate(fn, arg)

  // ---- chaves novas
  const novo = await ev(() => window.ponte.config.gravar({
    idiomaRespostas: 'pt-BR', tamanhoLetra: 110, animacoes: 'reduzidas', padraoNova: { modelo: 'haiku', esforco: 'BAIXO', rapido: false },
    lembrarAprovacoes: true, somAviso: false, volumeAviso: 30, cantoMini: 'se', abrirComWindows: true, comecarNaBandeja: false,
  }))
  const arq = JSON.parse(readFileSync(join(dados, 'config.json'), 'utf8'))
  conferir('grava as chaves novas e devolve a config', novo.tamanhoLetra === 110 && novo.padraoNova.esforco === 'low' && novo.cantoMini === 'se' && arq.miniCanto === 'superior-esquerdo' && arq.volumeAviso === 30 && arq.abrirComWindows === true, JSON.stringify(novo.padraoNova))
  const ruins = await ev(() => Promise.all([{ idioma: 'es' }, { tamanhoLetra: 111 }, { volumeAviso: 101 }, { cantoMini: 'meio' }, { padraoNova: { modelo: 'gpt' } }, { somAviso: 'sim' }, { abrirComWindows: 1 }, { modoConta: 'api' }]
    .map((x) => window.ponte.config.gravar(x).then(() => 'gravou', (e) => e.message))))
  conferir('recusa valores inválidos (e modo chave sem chave guardada)', ruins.every((x) => x !== 'gravou'), ruins.slice(-1)[0])
  await ev(() => window.ponte.config.gravar({ idioma: 'en' }))
  const emIngles = await ev(() => window.ponte.config.gravar({ volumeAviso: 500 }).then(() => '', (e) => e.message))
  conferir('mensagens no idioma da interface (en)', /from 0 to 100/.test(emIngles), emIngles)
  await ev(() => window.ponte.config.gravar({ idioma: 'pt-BR' }))
  const somMini = await ate(() => pag(/mini\.html/).evaluate(() => document.documentElement.dataset.tema !== undefined), 3000)
  conferir('as outras janelas recebem a config nova', !!somMini)

  // ---- cofre e conta (credencial de teste)
  const formatoRuim = await ev(() => window.ponte.conta.guardarChave('chave-qualquer').then(() => 'gravou', (e) => e.message))
  conferir('recusa chave fora do formato', formatoRuim !== 'gravou', formatoRuim)
  await ev((k) => window.ponte.conta.guardarChave(k), CHAVE_FALSA)
  const lista = spawnSync(CMDKEY, ['/list:' + ALVO], { encoding: 'utf8', windowsHide: true }).stdout
  conferir('guarda no Cofre do Windows com o nome de teste', lista.includes(ALVO))
  const teste = await ev(() => window.ponte.conta.testarChave())
  conferir('testarChave responde o erro da API sem mostrar a chave', teste && teste.ok === false && teste.status === 401 && !JSON.stringify(teste).includes('xxxxx'), JSON.stringify(teste))
  const st = await ev(() => window.ponte.conta.status())
  conferir('status: e-mail mascarado e temChave', st.temChave === true && (!st.email || /\*\*\*@/.test(st.email)), `${st.email} · modo ${st.modo}`)
  const api = await ev(() => window.ponte.config.gravar({ modoConta: 'api' }).then((c) => c.modoConta, (e) => e.message))
  conferir('modo chave de API liga com chave guardada', api === 'api')
  const sairSem = await ev(() => window.ponte.conta.sair(false).then(() => 'saiu', (e) => e.message))
  conferir('sair da conta exige confirmação (não deslogou)', sairSem !== 'saiu', sairSem)
  await ev(() => window.ponte.conta.apagarChave())
  const depois = await ev(() => window.ponte.conta.modo())
  const stDepois = await ev(() => window.ponte.conta.status())
  conferir('apagar a chave tira do Cofre e volta para a assinatura', depois === 'assinatura' && stDepois.temChave === false)
  const log = readFileSync(join(dados, 'ponte.log'), 'utf8') + readFileSync(join(dados, 'config.json'), 'utf8')
  conferir('a chave não aparece no registro nem na config', !log.includes(CHAVE_FALSA))

  // ---- idioma das respostas (Haiku de verdade)
  if (!semClaude) {
    await ev(() => window.ponte.config.gravar({ idiomaRespostas: 'en' }))
    const fala = async (texto) => {
      const fim = ev((t) => new Promise((ok) => {
        let txt = ''
        const desl = window.ponte.aoConversa((e) => { if (e.conversa !== 'idioma@1') return; if (e.tipo === 'texto') txt = e.texto; if (e.tipo === 'resultado' || e.tipo === 'erro') { desl(); ok(txt || e.texto || e.tipo) } })
        window.ponte.enviar({ conversa: 'idioma@1', texto: t }).catch((err) => ok('ERRO ' + err.message))
      }), texto)
      return Promise.race([fim, esperar(120000).then(() => 'sem resposta')])
    }
    const en = await fala('Explique em uma frase curta o que é um arquivo de texto.')
    conferir('idiomaRespostas "en": responde em inglês (append no system prompt)', /(^|[^a-z])(the|is|a|file|text)([^a-z]|$)/i.test(en) && !/(^|[^a-z])(é|um|arquivo|que)([^a-zà-ú]|$)/i.test(en), en.slice(0, 60))
    const info = await ev(() => window.ponte.conversa('idioma@1'))
    conferir('conversa nova usa o padrão (haiku, baixo)', info && info.modelo === 'haiku' && info.esforco === 'low')
    await ev(() => window.ponte.config.gravar({ idiomaRespostas: 'pt-BR' }))
    const pt = await fala('Explain in one short sentence what a folder is.')
    conferir('trocar para "pt-BR" vale já na mesma conversa (reabre retomando)', /(^|[^a-zà-ú])(é|uma|pasta|que|arquivos)([^a-zà-ú]|$)/i.test(pt), pt.slice(0, 60))
  }

  // ---- exportar / importar / restaurar
  const bom = join(base, 'bom.json'), ruim = join(base, 'ruim.json'), outro = join(base, 'outro.json')
  writeFileSync(bom, JSON.stringify({ programa: 'Ponte de Comando', versaoConfig: 1, config: { tema: 'grade', volumeAviso: 55, idioma: 'en' } }))
  writeFileSync(ruim, JSON.stringify({ programa: 'Ponte de Comando', versaoConfig: 1, config: { tema: 'grade', volumeAviso: 900 } }))
  writeFileSync(outro, JSON.stringify({ qualquer: 1 }))
  const imp = await ev((c) => window.ponte._invocar('sistema_importar_arquivo_teste', { caminho: c }), bom)
  const impRuim = await ev((c) => window.ponte._invocar('sistema_importar_arquivo_teste', { caminho: c }).then(() => 'aplicou', (e) => String(e)), ruim)
  const impOutro = await ev((c) => window.ponte._invocar('sistema_importar_arquivo_teste', { caminho: c }).then(() => 'aplicou', (e) => String(e)), outro)
  const aposRuim = await ev(() => window.ponte.config.ler())
  conferir('importar aplica o arquivo válido e recusa o inválido inteiro', imp.tema === 'grade' && impRuim !== 'aplicou' && impOutro !== 'aplicou' && aposRuim.volumeAviso === 55, impRuim)
  const semConfirmar = await ev(() => window.ponte.sistema.restaurarPadrao(false).then(() => 'restaurou', (e) => e.message))
  const restaurado = await ev(() => window.ponte.sistema.restaurarPadrao(true))
  conferir('restaurar padrão pede confirmação e volta tudo', semConfirmar !== 'restaurou' && restaurado.tema === 'metal' && restaurado.idioma === 'pt-BR' && restaurado.abrirComWindows === false, semConfirmar)
} catch (e) {
  conferir('o roteiro chegou ao fim', false, e.message)
} finally {
  if (nav) await nav.close().catch(() => {})
  fechar(prog)
  limparProcessos(base)
}

// ---- começar na bandeja: sem carregamento, sem principal, só a miniatura
try {
  prog = abrir({ comecarNaBandeja: true }, 9333)
  nav = await ate(() => chromium.connectOverCDP('http://127.0.0.1:9333'), 30000)
  const pag = (re) => nav.contexts().flatMap((c) => c.pages()).find((p) => re.test(p.url()))
  const main = await ate(() => pag(/Main\.dc\.html/), 20000)
  const j = await ate(() => main.evaluate(() => window.ponte.info().then((i) => i.janelas.mini && i.janelas)), 8000)
  conferir('começar na bandeja: sem carregamento, principal escondida, miniatura no canto', !!j && j.carregamento === null && j.principal === false, JSON.stringify(j))
} catch (e) {
  conferir('começar na bandeja', false, e.message)
} finally {
  if (nav) await nav.close().catch(() => {})
  fechar(prog)
  limparProcessos(base)
  // garantia: a credencial de teste nunca fica para trás
  spawnSync(CMDKEY, ['/delete:' + ALVO], { windowsHide: true })
}
const falhas = placar.filter((x) => !x).length
console.log(`\nplacar: ${placar.length - falhas}/${placar.length}`)
process.exit(falhas ? 1 : 0)
