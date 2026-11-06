// QA · a memória cresce a cada troca Main ⇄ Fluxo? Mede de 20 em 20 trocas (sem sessões do Claude).
// Uso: node ferramentas/testes/qa/vazamento.mjs [trocas=80]
import { mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate, medirArvore } from './comum.mjs'
const N = +(process.argv[2] || 80)
const falso = join(tmpdir(), 'ponte-qa-claude-vazio3'); rmSync(falso, { recursive: true, force: true }); mkdirSync(join(falso, 'sessions'), { recursive: true })
const app = await abrir('vazamento', { envExtra: { CLAUDE_CONFIG_DIR: falso } })
try {
  const main = app.main, serie = []
  await esperar(3000)
  serie.push(Math.round(medirArvore(app.prog.pid).memMB))
  for (let i = 1; i <= N; i++) {
    await main.evaluate(() => { location.href = 'Fluxo.dc.html' }); await ate(() => /Fluxo\.dc\.html/.test(main.url()), 8000, 100)
    await main.evaluate(() => { location.href = 'Main.dc.html' }); await ate(() => main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 15000, 150)
    if (i % 20 === 0) { await esperar(2000); serie.push(Math.round(medirArvore(app.prog.pid).memMB)) }
  }
  console.log('memória (MB) a cada 20 trocas:', serie.join(' → '))
  const passos = serie.slice(1).map((v, i) => v - serie[i])
  conferir(`depois das primeiras 20 trocas a memória para de crescer (passos: ${passos.join(', ')} MB)`, passos.slice(1).every((d) => d < 25), serie.join(' → '))
} catch (e) { conferir('roteiro', false, e.stack) } finally { await app.fechar(); rmSync(falso, { recursive: true, force: true }) }
process.exit(fimDoPlacar() ? 1 : 0)
