// QA · duas conversas do Claude Code ao mesmo tempo (hooks): o "pronto" de uma some quando a outra usa uma ferramenta?
// Uso: node ferramentas/testes/qa/duas-sessoes.mjs
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar } from './comum.mjs'
const app = await abrir('duas-sessoes')
try {
  const token = readFileSync(join(app.dados, 'token'), 'utf8').trim()
  const hook = (ev) => fetch(`http://127.0.0.1:${app.porta}/evento`, { method: 'POST', headers: { 'X-Ponte-Token': token, 'Content-Type': 'application/json' }, body: JSON.stringify(ev) })
  await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize()) // Alex fora da Main: o aviso é da miniatura
  await esperar(800)
  await hook({ tipo: 'pronto', sessao: 'sessao-A', projeto: 'curso-mirian', titulo: 'Faz a landing do curso' })
  await esperar(500)
  const a = await app.mini.evaluate(() => !document.getElementById('aviso').hidden && document.getElementById('aviso-titulo').textContent)
  await hook({ tipo: 'trabalhando', fase: 'ferramenta', sessao: 'sessao-B', projeto: 'valores', ferramenta: 'Read', legenda: 'Lendo LEIA-ME.md' })
  await esperar(500)
  const b = await app.mini.evaluate(() => !document.getElementById('aviso').hidden && document.getElementById('aviso-titulo').textContent)
  conferir('o aviso de pronto da conversa A continua quando a conversa B lê um arquivo', a && b, `antes: ${a || 'sem aviso'} · depois do Read da B: ${b || 'aviso sumiu'}`)
} catch (e) { conferir('roteiro', false, e.stack) } finally { await app.fechar() }
process.exit(fimDoPlacar() ? 1 : 0)
