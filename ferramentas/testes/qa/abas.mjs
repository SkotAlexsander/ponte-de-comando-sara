// QA · duas abas conversando ao mesmo tempo na Main (navegador comum, ponte de mentira, sem gasto).
// Uso: node ferramentas/testes/qa/abas.mjs [ponte-main.js]
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { servir } from '../../servir-ui.mjs'
import { chromium, raiz, conferir, fimDoPlacar, esperar } from './comum.mjs'
const MAIN = readFileSync(process.argv[2] || join(raiz, 'ui', 'ponte-main.js'), 'utf8')
const PONTE = readFileSync(join(raiz, 'ui', 'ponte.js'), 'utf8')
const srv = await servir(5198)
const nav = await chromium.launch()
try {
  const p = await nav.newPage({ viewport: { width: 2040, height: 790 } })
  await p.addInitScript(PONTE); await p.addInitScript(MAIN)
  // a ponte de mentira responde sozinha; aqui o enviar só registra, e os eventos saem quando o teste mandar
  await p.addInitScript(() => { const t = setInterval(() => { if (window.ponte) { clearInterval(t); window.__enviados = []; window.ponte.enviar = async (x) => { window.__enviados.push(x); return { conversa: x.conversa } }; window.__parados = []; window.ponte.parar = async (c) => { window.__parados.push(c); return true } } }, 1) })
  await p.goto('http://127.0.0.1:5198/estudio/Main.dc.html')
  await p.waitForFunction(() => window.__ponteMain && window.__ponteMain.ok, null, { timeout: 20000 })
  const L = () => 'window.__ponteMain.logica()'
  // aba A manda
  await p.fill('#msg', 'pedido da aba A'); await p.press('#msg', 'Enter')
  const chaveA = await p.evaluate(() => window.__enviados[0].conversa)
  // troca para a aba B (o que o clique na lista faz: muda st.sessao) e manda
  await p.evaluate(() => window.__ponteMain.logica().setState({ sessao: 'molduras' }))
  await esperar(200)
  await p.fill('#msg', 'pedido da aba B'); await p.press('#msg', 'Enter')
  const enviados = await p.evaluate(() => window.__enviados.map((x) => x.conversa))
  conferir('com a aba A trabalhando, a aba B consegue mandar', enviados.length === 2, JSON.stringify(enviados))
  const rodandoAntes = await p.evaluate(() => window.__ponteMain.logica().state.rodandoEm)
  // A termina
  await p.evaluate((c) => window.ponte._simular('ponte:conversa', { conversa: c, tipo: 'resultado', texto: 'A pronto', erro: false }), chaveA)
  await esperar(300)
  const rodandoDepois = await p.evaluate(() => window.__ponteMain.logica().state.rodandoEm)
  conferir('a aba A terminar não marca a aba B como parada', rodandoDepois === 'molduras', `rodandoEm: ${rodandoAntes} → ${rodandoDepois}`)
  // PARAR na aba B (ainda trabalhando no programa)
  await p.evaluate(() => { const b = document.querySelector('button.parar'); if (b) b.click() })
  const parados = await p.evaluate(() => window.__parados)
  conferir('PARAR na aba B chega ao programa', parados.length === 1, `parar chamado ${parados.length} vez(es) ${JSON.stringify(parados)}`)
} catch (e) { conferir('roteiro', false, e.stack) } finally { await nav.close(); srv.close() }
process.exit(fimDoPlacar() ? 1 : 0)
