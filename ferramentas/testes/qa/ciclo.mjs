// QA · ciclo de vida: instância única, fechar no meio do carregamento, rajada de minimizar/restaurar,
// CPU com tudo escondido, vazamento depois de muitas trocas e processos órfãos de conta()/conexoes().
// Uso: node ferramentas/testes/qa/ciclo.mjs
import { spawn, spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate, raiz, EXE, medirArvore, descendentes } from './comum.mjs'

// ---------- 1. fechar no meio do carregamento ----------
{
  const app = await abrir('carregamento', { semEsperarMain: true })
  try {
    const carreg = await ate(() => app.pagina(/carregamento\.html/), 15000)
    await esperar(1500)
    await carreg.evaluate(() => window.__TAURI__.window.getCurrentWindow().close()).catch(() => {})
    const main = await ate(() => app.pagina(/Main\.dc\.html/), 20000)
    const j = await ate(() => main.evaluate(() => window.ponte.info().then((i) => i.janelas.principal && i.janelas)), 20000)
    const dur = (Date.now() - app.t0) / 1000
    conferir('fechar o carregamento no meio: a principal abre mesmo assim e o programa não sai', !!j && app.saiu === null, `principal na tela ${dur.toFixed(1)} s depois de abrir`)
  } catch (e) { conferir('roteiro 1 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}

// ---------- 2. instância única, rajada, CPU, vazamento ----------
const app = await abrir('ciclo')
try {
  // a segunda abertura com os mesmos dados sai e traz a primeira
  await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize())
  await ate(() => app.info().then((i) => i.janelas.principalMinimizada), 4000)
  const t0 = Date.now()
  const segunda = spawn(EXE, [], { env: { ...app.env, PONTE_DEPURAR_PORTA: '9399' }, stdio: 'ignore', windowsHide: true })
  const codigo = await new Promise((ok) => { const tm = setTimeout(() => ok('não saiu em 10 s'), 10000); segunda.on('exit', (c) => { clearTimeout(tm); ok(c) }) })
  if (codigo === 'não saiu em 10 s') spawnSync('C:\\Windows\\System32\\taskkill.exe', ['/PID', String(segunda.pid), '/T', '/F'], { windowsHide: true })
  const voltou = await ate(() => app.info().then((i) => i.janelas.principal && !i.janelas.principalMinimizada), 5000)
  conferir('abrir duas vezes: a segunda sai sozinha e a primeira volta para a tela', codigo !== 'não saiu em 10 s' && !!voltou, `segunda saiu com ${codigo} em ${Date.now() - t0} ms; principal voltou=${!!voltou}`)

  // rajada sem pausa
  for (let i = 0; i < 20; i++) {
    await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize())
    await app.main.evaluate(() => window.ponte.mostrarPrincipal())
  }
  await esperar(400)
  let j = await app.info().then((i) => i.janelas)
  const cedo = j.principal && !j.principalMinimizada && j.mini === false
  await esperar(3500) // o vigia de 3 s
  j = await app.info().then((i) => i.janelas)
  conferir('rajada de 20 minimizar/restaurar sem pausa termina certa (principal na tela, miniatura escondida)', j.principal && !j.principalMinimizada && j.mini === false, `logo depois: ${cedo ? 'certo' : 'errado'} · depois do vigia: ${JSON.stringify(j)}`)

  // CPU: tudo na tela (fora da área visível) × tudo escondido (principal na bandeja, miniatura desligada)
  const amostra = async (ms) => { const a = medirArvore(app.prog.pid); await esperar(ms); const b = medirArvore(app.prog.pid); return { cpuPct: (100 * (b.cpu - a.cpu)) / (ms / 1000), memMB: b.memMB, processos: b.processos } }
  await esperar(1500)
  const naTela = await amostra(10000)
  await app.main.evaluate(() => window.ponte.config.gravar({ miniAoMinimizar: false }))
  await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().close())
  await ate(() => app.info().then((i) => !i.janelas.principal && !i.janelas.mini), 4000)
  await esperar(3000)
  const escondido = await amostra(10000)
  console.log(`CPU (de 1 núcleo) com a principal na tela: ${naTela.cpuPct.toFixed(1)} % · tudo escondido: ${escondido.cpuPct.toFixed(1)} %`)
  conferir('com tudo escondido a CPU fica parada (< 1 % de um núcleo)', escondido.cpuPct < 1, `${escondido.cpuPct.toFixed(1)} % (na tela: ${naTela.cpuPct.toFixed(1)} %)`)
  // miniatura sozinha (principal na bandeja)
  await app.main.evaluate(() => window.ponte.config.gravar({ miniAoMinimizar: true }))
  await ate(() => app.info().then((i) => i.janelas.mini), 4000)
  await esperar(2000)
  const soMini = await amostra(10000)
  console.log(`CPU com só a miniatura: ${soMini.cpuPct.toFixed(1)} %`)
  await app.main.evaluate(() => window.ponte.mostrarPrincipal())
  await ate(() => app.info().then((i) => i.janelas.principal), 4000)

  // vazamento: 30 idas e voltas Main ⇄ Fluxo e 400 eventos de hook
  const antes = medirArvore(app.prog.pid)
  const token = readFileSync(join(app.dados, 'token'), 'utf8').trim()
  let main = app.main
  for (let i = 0; i < 30; i++) {
    await main.evaluate(() => { location.href = 'Fluxo.dc.html' })
    await ate(() => /Fluxo\.dc\.html/.test(main.url()), 8000, 100)
    await main.evaluate(() => { location.href = 'Main.dc.html' })
    await ate(() => /Main\.dc\.html/.test(main.url()) && main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 15000, 150)
  }
  for (let i = 0; i < 400; i++) {
    const corpo = JSON.stringify({ tipo: i % 2 ? 'trabalhando' : 'esperando', fase: 'ferramenta', legenda: 'evento ' + i })
    await fetch(`http://127.0.0.1:${app.porta}/evento`, { method: 'POST', headers: { 'X-Ponte-Token': token, 'Content-Type': 'application/json' }, body: corpo }).catch(() => {})
  }
  await esperar(4000)
  const ouvintes = await main.evaluate(() => { let n = 0; for (const k of Object.keys(window)) if (/^_\d+$/.test(k)) n++; return n })
  const depois = medirArvore(app.prog.pid)
  const cresceu = depois.memMB - antes.memMB
  console.log(`memória (exe + WebView2): ${antes.memMB.toFixed(0)} → ${depois.memMB.toFixed(0)} MB · processos ${antes.processos} → ${depois.processos}`)
  conferir('30 trocas de página + 400 eventos: a memória não cresce mais que 60 MB', cresceu < 60, `${antes.memMB.toFixed(0)} → ${depois.memMB.toFixed(0)} MB (+${cresceu.toFixed(0)})`)
  conferir('30 trocas de página: o número de processos não cresce', depois.processos <= antes.processos, `${antes.processos} → ${depois.processos}`)
  // as idas e voltas trocam a "carga" da Main: a mesma aba vira outra conversa
  const c1 = await main.evaluate(() => window.__ponteMain.carga)
  await main.evaluate(() => { location.href = 'Fluxo.dc.html' }); await ate(() => /Fluxo/.test(main.url()), 8000)
  await main.evaluate(() => { location.href = 'Main.dc.html' }); await ate(() => main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 15000)
  const c2 = await main.evaluate(() => window.__ponteMain.carga)
  conferir('ir ao FLUXO e voltar mantém o id da conversa da aba (a mesma sessão do Claude)', c1 === c2, `landing@${c1} → landing@${c2}`)

  // conta() e conexoes(): não deixam processo para trás
  const antesProc = descendentes(app.prog.pid).map((x) => x.pid)
  const t1 = Date.now()
  const conta = await main.evaluate(() => window.ponte.conta())
  const cx = await main.evaluate(() => window.ponte.conexoes())
  const dur = ((Date.now() - t1) / 1000).toFixed(1)
  await esperar(3000)
  const sobra = descendentes(app.prog.pid).filter((x) => !antesProc.includes(x.pid) && !/msedgewebview2/i.test(x.nome))
  conferir('conta() e conexoes() respondem e não deixam processo filho para trás', conta && conta.ok && cx && cx.ok && sobra.length === 0, `${dur} s · conta.ok=${conta && conta.ok} · conexões=${cx && cx.conexoes && cx.conexoes.length} · sobraram: ${sobra.map((x) => x.nome + ' ' + x.pid).join(', ') || 'nada'}`)
} catch (e) { conferir('roteiro 2 chegou ao fim', false, e.stack) } finally { await app.fechar() }

process.exit(fimDoPlacar() ? 1 : 0)
