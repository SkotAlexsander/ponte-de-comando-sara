#!/usr/bin/env node
// Hook do Claude Code → Ponte de Comando · Sara.
// Lê o JSON do hook no stdin, traduz (mapear.mjs) e manda um POST para o programa em 127.0.0.1.
// Regras: nunca escreve no stdout (no UserPromptSubmit o stdout entraria no contexto do Claude),
// sai SEMPRE com código 0 e em no máximo ~2 s, mesmo com o programa fechado.
// Teste: node ferramentas/hook/ponte-hook.mjs --verboso < ferramentas/hook/exemplo-stop.json
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { request } from 'node:http'
import { mapear } from './mapear.mjs'

const verboso = process.argv.includes('--verboso') || process.env.PONTE_HOOK_VERBOSO === '1'
const dizer = (m) => { if (verboso) process.stderr.write(`ponte-hook: ${m}\n`) }
const sair = () => process.exit(0)
setTimeout(() => { dizer('demorou; saindo sem esperar'); sair() }, 2500).unref()

const pastaDeDados = process.env.PONTE_PASTA_DE_DADOS || join(process.env.APPDATA || '', 'com.pixelmartins.pontedecomando')

function lerStdin() {
  return new Promise((ok) => {
    let t = ''
    process.stdin.setEncoding('utf8')
    process.stdin.on('data', (d) => { t += d; if (t.length > 1_000_000) ok(t) })
    process.stdin.on('end', () => ok(t))
    process.stdin.on('error', () => ok(t))
  })
}

try {
  const bruto = await lerStdin()
  let entrada
  try { entrada = JSON.parse(bruto) } catch { dizer('stdin não é JSON'); sair() }
  const evento = mapear(entrada)
  if (!evento) { dizer(`evento ignorado: ${entrada && entrada.hook_event_name}`); sair() }
  let token, porta, idServidor
  try {
    token = readFileSync(join(pastaDeDados, 'token'), 'utf8').trim()
    const [p, id] = readFileSync(join(pastaDeDados, 'porta'), 'utf8').trim().split(/\s+/)
    porta = Number(p); idServidor = id || ''
  } catch { dizer(`o programa ainda não foi aberto (sem token/porta em ${pastaDeDados})`); sair() }
  // confere que quem está na porta é a Ponte desta execução, antes de mandar o token
  const ping = await new Promise((ok) => {
    const r = request({ host: '127.0.0.1', port: porta, path: '/ping', method: 'GET', timeout: 1000 }, (res) => {
      let t = ''; res.setEncoding('utf8'); res.on('data', (d) => { t += d; if (t.length > 2000) res.destroy() }); res.on('end', () => { try { ok(JSON.parse(t)) } catch { ok(null) } })
    })
    r.on('timeout', () => { r.destroy(); ok(null) }); r.on('error', () => ok(null)); r.end()
  })
  if (!idServidor || !ping || ping.app !== 'ponte-de-comando' || ping.id !== idServidor) {
    dizer(`quem responde na porta ${porta} não é esta Ponte de Comando; o token não foi enviado`)
    sair()
  }
  const corpo = Buffer.from(JSON.stringify(evento))
  const req = request({ host: '127.0.0.1', port: porta, path: '/evento', method: 'POST', timeout: 1500, headers: { 'Content-Type': 'application/json', 'Content-Length': corpo.length, 'X-Ponte-Token': token } }, (res) => {
    res.resume()
    res.on('end', () => { dizer(`enviado (${evento.tipo}${evento.fase ? '/' + evento.fase : ''}) → HTTP ${res.statusCode}`); sair() })
  })
  req.on('timeout', () => { dizer('o programa não respondeu'); req.destroy(); sair() })
  req.on('error', (e) => { dizer(`programa fechado ou porta errada (${porta}): ${e.code || e.message}`); sair() })
  req.end(corpo)
} catch (e) {
  dizer('falhou: ' + e.message)
  sair()
}
