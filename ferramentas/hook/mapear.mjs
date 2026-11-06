// Tradução do JSON de um hook do Claude Code para o evento da Ponte (sem rede, sem disco: testável).
import { readFileSync, openSync, readSync, fstatSync, closeSync, existsSync } from 'node:fs'
import { basename, isAbsolute, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

export function cortar(s, max) {
  const t = String(s ?? '').replace(/[\u0000-\u001f]+/g, ' ').replace(/\s+/g, ' ').trim()
  return t.length <= max ? t : t.slice(0, max - 1).trimEnd() + '…'
}

const arquivo = (p) => String(p || '').split(/[\\/]/).pop()

/** Mesmas frases da casca (src-tauri/src/conversa.rs → legenda_ferramenta). */
export function legendaFerramenta(nome, e = {}) {
  e = e || {}
  switch (nome) {
    case 'Read': case 'NotebookRead': return `Lendo ${arquivo(e.file_path)}`
    case 'Write': return `Escrevendo ${arquivo(e.file_path)}`
    case 'Edit': case 'MultiEdit': return `Editando ${arquivo(e.file_path)}`
    case 'NotebookEdit': return `Editando ${arquivo(e.notebook_path)}`
    case 'Bash': case 'PowerShell': return `Rodando: ${cortar(e.description || e.command, 60)}`
    case 'Grep': return `Procurando "${cortar(e.pattern, 40)}"`
    case 'Glob': return `Procurando arquivos ${cortar(e.pattern, 40)}`
    case 'Agent': case 'Task': return `Chamando ${e.subagent_type || 'um funcionário'}`
    case 'WebFetch': { const u = String(e.url || ''); return `Lendo a página ${(u.split('//')[1] || u).split('/')[0]}` }
    case 'WebSearch': return `Pesquisando "${cortar(e.query, 40)}"`
    case 'TodoWrite': return 'Organizando as tarefas'
    case 'Skill': return `Usando a skill ${e.skill || ''}`.trim()
    default:
      if (String(nome).startsWith('mcp__')) return `Usando ${String(nome).split('__').pop()}`
      return `Usando ${nome || 'uma ferramenta'}`
  }
}

/** Lê o fim do transcript e acha o último pedido de verdade e a última fala do Claude. */
export function lerFimDoTranscript(caminho, bytes = 512 * 1024) {
  const saida = { pedido: '', resposta: '' }
  if (!caminho) return saida
  let p = caminho
  if (!isAbsolute(p)) {
    const aqui = dirname(fileURLToPath(import.meta.url))
    p = [resolve(process.cwd(), caminho), resolve(aqui, caminho), resolve(aqui, '..', '..', caminho)].find(existsSync) || p
  }
  let texto = ''
  try {
    const fd = openSync(p, 'r')
    const tam = fstatSync(fd).size
    const ini = Math.max(0, tam - bytes)
    const buf = Buffer.alloc(tam - ini)
    readSync(fd, buf, 0, buf.length, ini)
    closeSync(fd)
    texto = buf.toString('utf8')
  } catch { return saida }
  const linhas = texto.split('\n').reverse()
  for (const l of linhas) {
    if (saida.pedido && saida.resposta) break
    let o
    try { o = JSON.parse(l) } catch { continue }
    if (!saida.resposta && o.type === 'assistant' && Array.isArray(o.message?.content)) {
      const t = o.message.content.filter((b) => b.type === 'text').map((b) => b.text).join(' ').trim()
      if (t) saida.resposta = t
    }
    if (!saida.pedido && o.type === 'user' && !o.isMeta) {
      const c = o.message?.content
      const t = typeof c === 'string' ? c : Array.isArray(c) ? c.filter((b) => b.type === 'text').map((b) => b.text).join(' ') : ''
      if (t && !t.trim().startsWith('<')) saida.pedido = t
    }
  }
  return saida
}

/** Entrada do hook → evento da Ponte (ou null para ignorar). */
export function mapear(h, { lerTranscript = lerFimDoTranscript } = {}) {
  if (!h || typeof h !== 'object') return null
  const base = { sessao: String(h.session_id || ''), projeto: h.cwd ? basename(String(h.cwd).replace(/[\\/]+$/, '')) : '' }
  switch (h.hook_event_name) {
    case 'UserPromptSubmit':
      return { ...base, tipo: 'trabalhando', fase: 'pedido', titulo: cortar(h.prompt, 140) }
    case 'PreToolUse':
      return { ...base, tipo: 'trabalhando', fase: 'ferramenta', ferramenta: String(h.tool_name || ''), legenda: cortar(legendaFerramenta(h.tool_name, h.tool_input), 120) }
    case 'PostToolUse':
      return { ...base, tipo: 'trabalhando', fase: 'ferramenta-fim', ferramenta: String(h.tool_name || ''), legenda: cortar(legendaFerramenta(h.tool_name, h.tool_input), 120) }
    case 'SubagentStop':
      return { ...base, tipo: 'trabalhando', fase: 'subagente', legenda: 'Um funcionário terminou a parte dele' }
    case 'Notification':
      return { ...base, tipo: 'precisa', texto: cortar(h.message || 'O Claude Code está esperando você.', 300) }
    case 'Stop': {
      const fim = lerTranscript(h.transcript_path)
      return { ...base, tipo: 'pronto', titulo: cortar(fim.pedido, 140), resumo: cortar(h.last_assistant_message || fim.resposta, 200) }
    }
    default:
      return null
  }
}

export function lerJson(caminho) {
  return JSON.parse(readFileSync(caminho, 'utf8'))
}
