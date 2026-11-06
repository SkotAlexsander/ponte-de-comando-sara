// QA · as páginas sabem que estão escondidas? (visibilityState, quadros/s, vídeos tocando, animações CSS)
// Uso: node ferramentas/testes/qa/visibilidade.mjs
import { mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate } from './comum.mjs'
const falso = join(tmpdir(), 'ponte-qa-claude-vazio2'); rmSync(falso, { recursive: true, force: true }); mkdirSync(join(falso, 'sessions'), { recursive: true })
const app = await abrir('visibilidade', { envExtra: { CLAUDE_CONFIG_DIR: falso } })
const olhar = (p) => p.evaluate(() => new Promise((ok) => {
  let n = 0; const t0 = performance.now(); const f = () => { n++; if (performance.now() - t0 < 2000) requestAnimationFrame(f) }; requestAnimationFrame(f)
  setTimeout(() => ok({ vis: document.visibilityState, hidden: document.hidden, quadrosPorS: Math.round(n / 2), videosTocando: [...document.querySelectorAll('video')].filter((v) => !v.paused).length, animacoes: document.getAnimations().filter((a) => a.playState === 'running').length }), 2300)
}))
try {
  await esperar(2000)
  console.log('na tela:', 'main', JSON.stringify(await olhar(app.main)))
  await app.main.evaluate(() => window.ponte.config.gravar({ miniAoMinimizar: false }))
  await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().close())
  await ate(() => app.info().then((i) => !i.janelas.principal && !i.janelas.mini), 4000)
  await esperar(2000)
  const m = await olhar(app.main), mi = await olhar(app.mini)
  console.log('escondido:', 'main', JSON.stringify(m), '· mini', JSON.stringify(mi))
  conferir('principal escondida: a página sabe (visibilityState hidden) e para vídeo e animação', m.vis === 'hidden' && m.videosTocando === 0 && m.animacoes === 0, JSON.stringify(m))
  conferir('miniatura escondida: a página sabe e para vídeo e animação', mi.vis === 'hidden' && mi.videosTocando === 0 && mi.animacoes === 0, JSON.stringify(mi))
} catch (e) { conferir('roteiro', false, e.stack) } finally { await app.fechar(); rmSync(falso, { recursive: true, force: true }) }
process.exit(fimDoPlacar() ? 1 : 0)
