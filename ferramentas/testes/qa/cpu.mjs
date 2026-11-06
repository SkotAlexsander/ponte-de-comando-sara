// QA · CPU por processo com as janelas escondidas (principal na bandeja, miniatura desligada).
// Uso: node ferramentas/testes/qa/cpu.mjs [--claude-vazio]   (--claude-vazio: CLAUDE_CONFIG_DIR falso, sem sessões)
import { mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate, ps, descendentes } from './comum.mjs'

const vazio = process.argv.includes('--claude-vazio')
const falso = join(tmpdir(), 'ponte-qa-claude-vazio')
if (vazio) { rmSync(falso, { recursive: true, force: true }); mkdirSync(join(falso, 'sessions'), { recursive: true }); mkdirSync(join(falso, 'projects'), { recursive: true }) }
const app = await abrir('cpu', { envExtra: vazio ? { CLAUDE_CONFIG_DIR: falso } : {} })
const porProcesso = () => {
  const ids = [app.prog.pid, ...descendentes(app.prog.pid).map((x) => x.pid)]
  const s = ps(`foreach($i in @(${ids.join(',')})){ $p=Get-Process -Id $i -ErrorAction SilentlyContinue; if($p){ $c=(Get-CimInstance Win32_Process -Filter "ProcessId=$i").CommandLine; $t=if($c -match '--type=([a-z-]+)'){$matches[1]}else{'-'}; "$i|$($p.ProcessName)|$t|$($p.TotalProcessorTime.TotalSeconds)|$([int]($p.PrivateMemorySize64/1MB))" } }`)
  return Object.fromEntries(s.split(/\r?\n/).filter(Boolean).map((l) => { const [pid, nome, tipo, cpu, mem] = l.split('|'); return [pid, { nome, tipo, cpu: +cpu, mem: +mem }] }))
}
const medir = async (rotulo, ms = 10000) => {
  const a = porProcesso(); await esperar(ms); const b = porProcesso()
  let total = 0
  const linhas = Object.entries(b).map(([pid, x]) => { const pct = a[pid] ? (100 * (x.cpu - a[pid].cpu)) / (ms / 1000) : NaN; if (!isNaN(pct)) total += pct; return `  ${pid} ${x.nome} ${x.tipo}: ${isNaN(pct) ? 'novo' : pct.toFixed(1) + ' %'} · ${x.mem} MB` })
  console.log(`${rotulo}: ${total.toFixed(1)} % de um núcleo\n${linhas.join('\n')}`)
  return total
}
try {
  await esperar(3000)
  const naTela = await medir('principal na tela (fora da área visível)')
  await app.main.evaluate(() => window.ponte.config.gravar({ miniAoMinimizar: false }))
  await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().close())
  await ate(() => app.info().then((i) => !i.janelas.principal && !i.janelas.mini), 4000)
  // (06/11, Davi: o WebView2 leva uns segundos para soltar a GPU depois de esconder; mede em regime, 20 s depois)
  await esperar(20000)
  const esc = await medir('tudo escondido')
  conferir(`tudo escondido${vazio ? ' (sem sessões do Claude)' : ''}: CPU < 2 % de um núcleo`, esc < 2, `${esc.toFixed(1)} % (na tela ${naTela.toFixed(1)} %)`)
} catch (e) { conferir('o roteiro chegou ao fim', false, e.stack) } finally {
  await app.fechar()
  if (vazio) rmSync(falso, { recursive: true, force: true })
}
process.exit(fimDoPlacar() ? 1 : 0)
