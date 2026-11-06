// node --test ferramentas/testes/
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { createServer } from 'node:http'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mapear, legendaFerramenta, lerJson } from '../hook/mapear.mjs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const ex = (n) => lerJson(join(raiz, 'ferramentas', 'hook', n))

test('UserPromptSubmit vira trabalhando com o título do pedido', () => {
  const e = mapear(ex('exemplo-prompt.json'))
  assert.equal(e.tipo, 'trabalhando'); assert.equal(e.fase, 'pedido')
  assert.match(e.titulo, /landing do curso/); assert.equal(e.projeto, 'curso-mirian')
})

test('PreToolUse vira trabalhando com a ferramenta em português', () => {
  const e = mapear(ex('exemplo-pretool.json'))
  assert.equal(e.legenda, 'Lendo LEIA-ME.md'); assert.equal(e.ferramenta, 'Read')
  assert.equal(legendaFerramenta('Bash', { description: 'Roda os testes' }), 'Rodando: Roda os testes')
  assert.equal(legendaFerramenta('mcp__canva__create-design', {}), 'Usando create-design')
})

test('Notification vira precisa de você; SubagentStop segue trabalhando', () => {
  assert.equal(mapear(ex('exemplo-notification.json')).tipo, 'precisa')
  assert.equal(mapear(ex('exemplo-subagentstop.json')).fase, 'subagente')
})

test('Stop vira pronto com o pedido e o resumo tirados do transcript', () => {
  const e = mapear(ex('exemplo-stop.json'))
  assert.equal(e.tipo, 'pronto')
  assert.match(e.titulo, /landing do curso da Mirian/)
  assert.equal(e.resumo, 'Montei as telas A e B e o texto do topo.')
})

test('evento desconhecido é ignorado', () => {
  assert.equal(mapear({ hook_event_name: 'SessionStart' }), null)
  assert.equal(mapear(null), null)
})

test('o hook manda o POST com o token, não escreve no stdout e sai com 0', async () => {
  const dados = mkdtempSync(join(tmpdir(), 'ponte-hook-'))
  const recebidos = []
  const srv = createServer((req, res) => {
    if (req.url === '/ping') { res.end('{"ok":true,"app":"ponte-de-comando","id":"id-teste"}'); return }
    let c = ''; req.on('data', (d) => c += d); req.on('end', () => { recebidos.push({ token: req.headers['x-ponte-token'], corpo: JSON.parse(c) }); res.end('{"ok":true}') })
  })
  await new Promise((ok) => srv.listen(0, '127.0.0.1', ok))
  writeFileSync(join(dados, 'token'), 'abc123'); writeFileSync(join(dados, 'porta'), `${srv.address().port} id-teste`)
  const r = await new Promise((ok) => {
    const { spawn } = require_spawn()
    const p = spawn(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { cwd: raiz, windowsHide: true, env: { ...process.env, PONTE_PASTA_DE_DADOS: dados } })
    let out = ''; p.stdout.on('data', (d) => out += d)
    p.on('exit', (code) => ok({ code, out }))
    p.stdin.end(JSON.stringify(ex('exemplo-stop.json')))
  })
  srv.close()
  assert.equal(r.code, 0); assert.equal(r.out, '')
  assert.equal(recebidos.length, 1); assert.equal(recebidos[0].token, 'abc123'); assert.equal(recebidos[0].corpo.tipo, 'pronto')
})

test('outro programa na porta (sem o id certo) não recebe o token', async () => {
  const dados = mkdtempSync(join(tmpdir(), 'ponte-hook-intruso-'))
  const vistos = []
  const srv = createServer((req, res) => { vistos.push(req.headers['x-ponte-token'] || ''); res.end('{"ok":true,"id":"outro"}') })
  await new Promise((ok) => srv.listen(0, '127.0.0.1', ok))
  writeFileSync(join(dados, 'token'), 'segredo-de-teste'); writeFileSync(join(dados, 'porta'), `${srv.address().port} id-certo`)
  const r = spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { input: JSON.stringify(ex('exemplo-stop.json')), windowsHide: true, env: { ...process.env, PONTE_PASTA_DE_DADOS: dados } })
  srv.close()
  assert.equal(r.status, 0)
  assert.ok(!vistos.includes('segredo-de-teste'))
})

test('com o programa fechado o hook sai com 0, rápido e calado', () => {
  const dados = mkdtempSync(join(tmpdir(), 'ponte-hook-fechado-'))
  writeFileSync(join(dados, 'token'), 'x'); writeFileSync(join(dados, 'porta'), '1')
  const t0 = Date.now()
  const r = spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { input: JSON.stringify(ex('exemplo-stop.json')), windowsHide: true, env: { ...process.env, PONTE_PASTA_DE_DADOS: dados } })
  assert.equal(r.status, 0); assert.equal(String(r.stdout), ''); assert.ok(Date.now() - t0 < 3000)
  const semDados = spawnSync(process.execPath, [join(raiz, 'ferramentas', 'hook', 'ponte-hook.mjs')], { input: 'lixo', windowsHide: true, env: { ...process.env, PONTE_PASTA_DE_DADOS: join(dados, 'nao-existe') } })
  assert.equal(semDados.status, 0)
})

import * as cp from 'node:child_process'
function require_spawn() { return cp }
