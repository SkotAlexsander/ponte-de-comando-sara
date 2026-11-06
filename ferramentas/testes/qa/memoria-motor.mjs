// QA · pico de memória do exe (só ele, sem o WebView2) enquanto o motor lê as sessões vivas REAIS (só leitura).
// Uso: node ferramentas/testes/qa/memoria-motor.mjs
import { abrir, conferir, fimDoPlacar, esperar, ps } from './comum.mjs'
const app = await abrir('memoria-motor')
try {
  let pico = 0; const t0 = Date.now(); const serie = []
  while (Date.now() - t0 < (+process.argv[2] || 40) * 1000) { const m = +ps(`(Get-Process -Id ${app.prog.pid}).PrivateMemorySize64`) / 1048576; pico = Math.max(pico, m); serie.push(Math.round(m)); await esperar(1500) }
  const t = await app.main.evaluate(() => window.ponte.tokens()); const fimMem = +ps(`(Get-Process -Id ${app.prog.pid}).PrivateMemorySize64`) / 1048576; console.log('memória no fim:', fimMem.toFixed(0), 'MB'); const prim = serie.findIndex(() => false)
  console.log('memória privada do exe (MB):', serie.join(' '))
  conferir('lendo as sessões vivas reais, o exe fica abaixo de 150 MB', pico < 150, `pico ${pico.toFixed(0)} MB · ${t.sessoes.length} sessão(ões) viva(s), ${t.sessoes.filter((s) => s.respostas).length} medida(s)`)
} catch (e) { conferir('roteiro', false, e.stack) } finally { await app.fechar() }
process.exit(fimDoPlacar() ? 1 : 0)
