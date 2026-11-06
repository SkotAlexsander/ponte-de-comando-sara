// O append de idioma chega ao Claude? Manda a mesma pergunta em português com instrução "en" (Haiku, centavos).
// Uso: node ferramentas/testes/sidecar-idioma.mjs
import { spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { randomUUID } from 'node:crypto'
import { fileURLToPath } from 'node:url'
const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const instrucao = process.argv[2] || 'Language setting chosen by the user in Ponte de Comando: write every reply in English, even when the user writes in Portuguese or another language.'
const cwd = mkdtempSync(join(tmpdir(), 'ponte-idioma-'))
const p = spawn(process.execPath, [join(raiz, 'sidecar', 'ponte-sidecar.mjs')], { stdio: ['pipe', 'pipe', 'inherit'], windowsHide: true })
let buf = '', texto = ''
p.stdout.on('data', (d) => {
  buf += d; let k
  while ((k = buf.indexOf('\n')) >= 0) {
    const e = JSON.parse(buf.slice(0, k)); buf = buf.slice(k + 1)
    if (e.evento === 'pronto_para_ouvir') p.stdin.write(JSON.stringify({ cmd: 'enviar', conversa: 'i', texto: 'Explique em uma frase curta o que é um arquivo de texto.', modelo: 'haiku', esforco: 'low', cwd, sessao: randomUUID(), retomar: false, instrucao, language: 'english', marcaIdioma: '[Ponte de Comando: answer in English]' }) + '\n')
    if (e.tipo === 'texto') texto = e.texto
    if (e.tipo === 'resultado' || e.tipo === 'erro') { console.log('resposta:', texto || e.texto); p.stdin.write('{"cmd":"sair"}\n') }
  }
})
