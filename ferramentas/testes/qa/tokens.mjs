// QA · motor de tokens com um ~/.claude FALSO (CLAUDE_CONFIG_DIR): transcript gigante, linha truncada,
// ~/.claude.json sem cachedUsageUtilization. Nada do ~/.claude real é lido.
// Uso: node ferramentas/testes/qa/tokens.mjs [MB=60]
import { spawn } from 'node:child_process'
import { mkdirSync, writeFileSync, openSync, writeSync, closeSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ps } from './comum.mjs'

const MB = +(process.argv[2] || 60)
const falso = join(tmpdir(), 'ponte-qa-claude-falso')
rmSync(falso, { recursive: true, force: true })
const trabalho = join(tmpdir(), 'ponte-qa-tokens', 'trabalho') // a pasta que o abrir() cria
const cod = trabalho.replace(/[^a-zA-Z0-9]/g, '-')
mkdirSync(join(falso, 'sessions'), { recursive: true })
mkdirSync(join(falso, 'projects', cod), { recursive: true })
writeFileSync(join(falso, '.claude.json'), JSON.stringify({ numStartups: 3, oauthAccount: {} })) // sem cachedUsageUtilization

// um processo vivo para fazer de "Claude Code aberto"
const dorminhoco = spawn(process.execPath, ['-e', 'setInterval(()=>{},1e9)'], { stdio: 'ignore', windowsHide: true })
const sessao = '11111111-2222-4333-8444-555555555555'
writeFileSync(join(falso, 'sessions', `${dorminhoco.pid}.json`), JSON.stringify({ pid: dorminhoco.pid, sessionId: sessao, cwd: trabalho, status: 'idle', name: 'qa gigante', updatedAt: Date.now() }))

// transcript: linhas curtas de saída de ferramenta (o caso comum) e uma resposta com usage a cada 200
const arq = join(falso, 'projects', cod, `${sessao}.jsonl`)
const fd = openSync(arq, 'w')
const curta = JSON.stringify({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'x', content: 'r'.repeat(180) }] } }) + '\n'
let bytes = 0, respostas = 0, i = 0
const blocos = []
while (bytes < MB * 1048576) {
  let l
  if (i % 200 === 0) { respostas++; l = JSON.stringify({ type: 'assistant', timestamp: '2026-11-06T10:00:00Z', message: { id: 'msg_' + respostas, model: 'claude-haiku-4-5', usage: { input_tokens: 10, output_tokens: 5, cache_read_input_tokens: 100 } } }) + '\n' }
  else if (i === 5000) l = '{"type":"assistant","message":{"id":"truncada","usage":{"input_tok\n' // linha cortada no meio (queda do CLI)
  else l = curta
  blocos.push(l); bytes += l.length; i++
  if (blocos.length >= 5000) { writeSync(fd, blocos.join('')); blocos.length = 0 }
}
writeSync(fd, blocos.join('')); writeSync(fd, '{"type":"assistant","message":{"id":"ultima-sem-fim"'); closeSync(fd)
console.log(`transcript falso: ${(bytes / 1048576).toFixed(0)} MB, ${i} linhas, ${respostas} respostas com usage`)

const app = await abrir('tokens', { envExtra: { CLAUDE_CONFIG_DIR: falso } })
try {
  const t0 = Date.now()
  let pico = 0, medido = null
  while (Date.now() - t0 < 240000) {
    const mem = +ps(`(Get-Process -Id ${app.prog.pid} -ErrorAction SilentlyContinue).PrivateMemorySize64`) / 1048576
    pico = Math.max(pico, mem || 0)
    medido = await app.main.evaluate(() => window.ponte.tokens()).catch(() => null)
    const s = medido && medido.sessoes.find((x) => x.respostas)
    if (s) { medido = s; break }
    await esperar(400)
  }
  const seg = (Date.now() - t0) / 1000
  const s = medido && medido.respostas ? medido : null
  console.log(`medido em ${seg.toFixed(1)} s · pico de memória privada do exe ${pico.toFixed(0)} MB · respostas=${s && s.respostas}`)
  conferir('transcript gigante com linha truncada: conta todas as respostas e nenhuma inventada', !!s && s.respostas === respostas, `${s && s.respostas} de ${respostas}`)
  conferir(`transcript de ${MB} MB medido em menos de 15 s`, !!s && seg < 15, `${seg.toFixed(1)} s`)
  conferir(`memória do exe não passa de ~${MB} MB por causa de um transcript de ${MB} MB`, pico < 150 + MB * 0.5, `pico ${pico.toFixed(0)} MB`)
  const lim = await app.main.evaluate(() => window.ponte.limites())
  conferir('~/.claude.json sem cachedUsageUtilization: limites() responde "indisponível" sem quebrar', lim && lim.disponivel === false, JSON.stringify(lim))
} catch (e) {
  conferir('o roteiro chegou ao fim', false, e.stack)
} finally {
  await app.fechar()
  dorminhoco.kill()
  rmSync(falso, { recursive: true, force: true })
}
process.exit(fimDoPlacar() ? 1 : 0)
