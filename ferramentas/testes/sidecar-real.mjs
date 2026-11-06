// Teste do sidecar com o Claude de verdade (gasta pouquíssimo: Haiku, esforço baixo, respostas de 1 palavra).
// Uso: node ferramentas/testes/sidecar-real.mjs
import { spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { randomUUID } from 'node:crypto'
import { fileURLToPath } from 'node:url'
const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const cwd = mkdtempSync(join(tmpdir(), 'ponte-sidecar-'))
const sessao = randomUUID()
function rodar(passos) {
  return new Promise((ok) => {
    const p = spawn(process.execPath, [join(raiz, 'sidecar', 'ponte-sidecar.mjs')], { stdio: ['pipe', 'pipe', 'inherit'] })
    const evs = []; let buf = ''; let i = 0
    const proximo = () => { const s = passos[i++]; if (!s) { p.stdin.write(JSON.stringify({ cmd: 'sair' }) + '\n'); return } p.stdin.write(JSON.stringify(s) + '\n') }
    p.stdout.on('data', (d) => {
      buf += d; let k
      while ((k = buf.indexOf('\n')) >= 0) {
        const l = buf.slice(0, k); buf = buf.slice(k + 1)
        const e = JSON.parse(l); evs.push(e)
        if (e.evento === 'pronto_para_ouvir') proximo()
        if (e.evento === 'permissao') p.stdin.write(JSON.stringify({ cmd: 'permissao', pedido: e.pedido, decisao: 'negar' }) + '\n')
        if (e.tipo === 'resultado' || (e.tipo === 'erro')) proximo()
      }
    })
    p.on('exit', () => ok(evs))
  })
}
const c = { cmd: 'enviar', conversa: 't1', modelo: 'haiku', esforco: 'low', rapido: false, cwd, sessao }
const a = await rodar([{ ...c, texto: 'Responda apenas a palavra: AZUL', retomar: false }, { ...c, texto: 'Qual palavra você respondeu antes? Responda só ela.', esforco: 'medium' }])
const b = await rodar([{ ...c, texto: 'Repita só a palavra que você respondeu na primeira mensagem.', retomar: true }])
const resumo = (evs) => evs.filter((e) => e.evento === 'pronto_para_ouvir' || ['inicio', 'resultado', 'erro'].includes(e.tipo)).map((e) => e.evento === 'pronto_para_ouvir' ? `sdk ${e.versaoSdk} claude=${e.claude}` : `${e.tipo} ${e.sessao || ''} ${e.modelo || ''} ${(e.texto || '').slice(0, 80)} ${e.custo ?? ''}`)
console.log('processo 1:\n ' + resumo(a).join('\n '))
console.log('deltas no processo 1:', a.filter((e) => e.tipo === 'delta').length)
console.log('processo 2 (retomado):\n ' + resumo(b).join('\n '))
const ok = a.filter((e) => e.tipo === 'resultado' && !e.erro).length === 2 && b.some((e) => e.tipo === 'resultado' && /azul/i.test(e.texto)) && a.find((e) => e.tipo === 'inicio')?.sessao === sessao
console.log(ok ? 'PASSOU' : 'FALHOU')
process.exit(ok ? 0 : 1)
