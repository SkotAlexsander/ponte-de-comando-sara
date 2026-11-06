// Sidecar da Ponte de Comando: conversa com o Claude Code pelo Claude Agent SDK oficial
// (@anthropic-ai/claude-agent-sdk) e fala com a casca Rust por JSON, uma linha por mensagem.
//
//   Rust → sidecar (stdin):
//     {"cmd":"enviar","conversa":"c1","texto":"...","modelo":"opus","esforco":"high","rapido":false,
//      "cwd":"A:\\Claude","sessao":"<uuid>","retomar":false}
//     {"cmd":"parar","conversa":"c1"}            interrompe o turno sem perder a conversa
//     {"cmd":"fechar","conversa":"c1"}           encerra a sessão viva (retomável depois)
//     {"cmd":"permissao","pedido":"p7","decisao":"permitir"|"negar","motivo":"...","lembrar":true}
//   "enviar" pode trazer "instrucao": texto curto que vai no FIM do system prompt do Claude Code
//   (preset claude_code + append; o prompt do Claude Code nunca é trocado). Ex.: "Always answer the user in English."
//     {"cmd":"parar_agente","conversa":"c1","tarefa":"<task_id>"}
//     {"cmd":"sair"}
//   sidecar → Rust (stdout):
//     {"evento":"pronto_para_ouvir","versaoSdk":"..."}
//     {"evento":"conversa","conversa":"c1","tipo":"inicio"|"delta"|"texto"|"ferramenta"|"resultado"|"erro"|"cancelado"|"agente"|"limite", ...}
//     {"evento":"permissao","conversa":"c1","pedido":"p7","ferramenta":"Bash","entrada":{...}}
//
// Regras: modo de permissão SEMPRE 'default' (cada ação passa pelo cartão do Alex);
// carrega os settings do usuário, do projeto e locais (CLAUDE.md, deny, hooks valem);
// stdout só leva JSON (todo log vai para stderr).
import { createInterface } from 'node:readline'
import { query } from '@anthropic-ai/claude-agent-sdk'
import { existsSync, readFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const aqui = dirname(fileURLToPath(import.meta.url))
const versaoSdk = (() => {
  try { return JSON.parse(readFileSync(join(aqui, 'node_modules/@anthropic-ai/claude-agent-sdk/package.json'), 'utf8')).version } catch { return '?' }
})()

console.log = (...a) => process.stderr.write(a.join(' ') + '\n')
const enviarRust = (obj) => process.stdout.write(JSON.stringify(obj) + '\n')
const log = (texto) => process.stderr.write(`[sidecar] ${texto}\n`)

function exeClaude() {
  if (process.env.PONTE_CLAUDE_EXE && existsSync(process.env.PONTE_CLAUDE_EXE)) return process.env.PONTE_CLAUDE_EXE
  const p = join(process.env.USERPROFILE || '', '.local', 'bin', 'claude.exe')
  return existsSync(p) ? p : undefined // undefined = o executável que vem com o SDK (se instalado)
}

/** Fila assíncrona: o SDK lê as mensagens do Alex daqui (modo de entrada contínua). */
function criarFila() {
  const itens = []
  let acordar = null
  let fechada = false
  return {
    empurrar(x) { itens.push(x); if (acordar) { acordar(); acordar = null } },
    fechar() { fechada = true; if (acordar) { acordar(); acordar = null } },
    async *[Symbol.asyncIterator]() {
      while (true) {
        if (itens.length) { yield itens.shift(); continue }
        if (fechada) return
        await new Promise((r) => { acordar = r })
      }
    },
  }
}

const conversas = new Map() // id → { q, fila, cfg, sessao, ocupada }
const pendentes = new Map() // pedido → { resolver, conversa }
let contadorPedido = 0

const cortarEntrada = (entrada) => {
  // a interface só precisa do bastante para a frase do cartão
  const s = JSON.stringify(entrada ?? {})
  if (s.length <= 4000) return entrada
  const r = {}
  for (const [k, v] of Object.entries(entrada || {})) r[k] = typeof v === 'string' ? v.slice(0, 1500) : v
  return r
}

function traduzir(conversa, msg, estado) {
  const base = { evento: 'conversa', conversa }
  switch (msg.type) {
    case 'system':
      if (msg.subtype === 'init') {
        estado.sessao = msg.session_id
        return [{ ...base, tipo: 'inicio', sessao: msg.session_id, modelo: msg.model, permissao: msg.permissionMode }]
      }
      if (['task_started', 'task_progress', 'task_updated', 'task_notification'].includes(msg.subtype)) {
        return [{ ...base, tipo: 'agente', subtipo: msg.subtype, tarefa: msg.task_id, descricao: msg.description || msg.summary || '', estadoTarefa: msg.status || '', pai: msg.parent_tool_use_id || msg.tool_use_id || null, uso: msg.usage || null, tipoTarefa: msg.task_type || '' }]
      }
      return []
    case 'stream_event': {
      const ev = msg.event || {}
      if (ev.type === 'content_block_delta' && ev.delta?.type === 'text_delta' && !msg.parent_tool_use_id) {
        return [{ ...base, tipo: 'delta', texto: ev.delta.text }]
      }
      return []
    }
    case 'assistant': {
      const saida = []
      const pai = msg.parent_tool_use_id || null
      for (const b of msg.message?.content || []) {
        if (b.type === 'text' && b.text?.trim() && !pai) saida.push({ ...base, tipo: 'texto', texto: b.text.trim() })
        if (b.type === 'tool_use') saida.push({ ...base, tipo: 'ferramenta', nome: b.name, id: b.id, entrada: cortarEntrada(b.input), pai })
      }
      return saida
    }
    case 'result':
      return [{ ...base, tipo: 'resultado', texto: msg.result ?? '', erro: !!msg.is_error || msg.subtype !== 'success', subtipo: msg.subtype, sessao: msg.session_id, custo: msg.total_cost_usd, duracaoMs: msg.duration_ms, tokensEntrada: msg.usage?.input_tokens, tokensSaida: msg.usage?.output_tokens }]
    case 'rate_limit_event':
      return [{ ...base, tipo: 'limite', info: msg.rate_limit_info || msg }]
    default:
      return []
  }
}

async function abrir(c) {
  const fila = criarFila()
  const estado = { sessao: c.sessao, ocupada: false, cfg: { modelo: c.modelo, esforco: c.esforco, rapido: !!c.rapido }, instrucao: c.instrucao || '', language: c.language || '', fila, q: null, cauda: '' }
  const opcoes = {
    cwd: c.cwd,
    model: c.modelo,
    effort: c.esforco,
    // idioma das respostas: a configuração oficial "language" do Claude Code + o append (reforço)
    settings: c.language ? { fastMode: !!c.rapido, language: c.language } : { fastMode: !!c.rapido },
    permissionMode: 'default',
    // sempre o prompt do Claude Code; a instrução de idioma só entra no fim (snapshot false: trocar o idioma vale já)
    systemPrompt: c.instrucao ? { type: 'preset', preset: 'claude_code', append: c.instrucao, snapshot: false } : { type: 'preset', preset: 'claude_code' },
    settingSources: ['user', 'project', 'local'],
    includePartialMessages: true,
    agentProgressSummaries: false, // custaria caro por agente (decisão copiada do Oficina)
    pathToClaudeCodeExecutable: exeClaude(),
    env: Object.fromEntries(Object.entries(process.env).filter(([k]) => k !== 'CLAUDECODE' && k !== 'CLAUDE_CODE_ENTRYPOINT')),
    stderr: (d) => { estado.cauda = (estado.cauda + d).slice(-3000) },
    canUseTool: (ferramenta, entrada, { signal, suggestions }) => new Promise((resolver) => {
      const pedido = 'p' + (++contadorPedido)
      pendentes.set(pedido, { resolver, conversa: c.conversa, entrada, sugestoes: suggestions })
      signal?.addEventListener('abort', () => {
        if (pendentes.delete(pedido)) resolver({ behavior: 'deny', message: 'Pedido cancelado.' })
        enviarRust({ evento: 'permissao_fim', conversa: c.conversa, pedido })
      })
      enviarRust({ evento: 'permissao', conversa: c.conversa, pedido, ferramenta, entrada: cortarEntrada(entrada) })
    }),
  }
  if (c.retomar) opcoes.resume = c.sessao
  else opcoes.sessionId = c.sessao
  estado.q = query({ prompt: fila, options: opcoes })
  conversas.set(c.conversa, estado)
  ;(async () => {
    try {
      for await (const msg of estado.q) {
        for (const ev of traduzir(c.conversa, msg, estado)) {
          if (ev.tipo === 'resultado') estado.ocupada = false
          enviarRust(ev)
        }
      }
    } catch (e) {
      if (estado.substituida) return // fechada de propósito para reabrir (idioma mudou)
      const texto = String(e?.message || e) + (estado.cauda ? `\n${estado.cauda.trim().slice(-600)}` : '')
      log(`conversa ${c.conversa} caiu: ${texto}`)
      enviarRust({ evento: 'conversa', conversa: c.conversa, tipo: 'erro', texto })
    } finally {
      // se outra sessão já tomou o lugar desta, ela é quem fala pela conversa
      if (!estado.substituida && conversas.get(c.conversa) === estado) {
        conversas.delete(c.conversa)
        enviarRust({ evento: 'conversa', conversa: c.conversa, tipo: 'fechada' })
      }
    }
  })()
  return estado
}

async function enviar(c) {
  let estado = conversas.get(c.conversa)
  // o idioma das respostas mudou: a instrução é opção de abertura, então reabre a sessão (retomando)
  if (estado && ((c.instrucao || '') !== estado.instrucao || (c.language || '') !== estado.language) && !estado.ocupada) {
    estado.substituida = true
    estado.fila.fechar(); try { estado.q.close() } catch {}
    conversas.delete(c.conversa)
    estado = null
    c = { ...c, retomar: true }
  }
  if (!estado) estado = await abrir(c)
  else {
    // a mesma sessão viva, com o modelo/esforço/velocidade que a aba pede agora
    const cfg = { modelo: c.modelo, esforco: c.esforco, rapido: !!c.rapido }
    if (cfg.modelo !== estado.cfg.modelo) await estado.q.setModel(cfg.modelo)
    if (cfg.esforco !== estado.cfg.esforco || cfg.rapido !== estado.cfg.rapido) await estado.q.applyFlagSettings({ effortLevel: cfg.esforco, fastMode: cfg.rapido })
    estado.cfg = cfg
  }
  estado.ocupada = true
  // idioma das respostas: a marca curta na mensagem é o que o Claude Code obedece (o append sozinho perde
  // para a língua em que o Alex escreve); a tela mostra só o que ele digitou
  const conteudo = c.marcaIdioma ? `${c.marcaIdioma}

${c.texto}` : c.texto
  estado.fila.empurrar({ type: 'user', message: { role: 'user', content: conteudo }, parent_tool_use_id: null, session_id: estado.sessao || '' })
}

async function tratar(cmd) {
  switch (cmd.cmd) {
    case 'enviar':
      try { await enviar(cmd) } catch (e) { enviarRust({ evento: 'conversa', conversa: cmd.conversa, tipo: 'erro', texto: String(e?.message || e) }) }
      break
    case 'parar': {
      const e = conversas.get(cmd.conversa)
      if (!e) break
      for (const [id, p] of pendentes) if (p.conversa === cmd.conversa) { pendentes.delete(id); p.resolver({ behavior: 'deny', message: 'O Alex parou.' }) }
      try { await e.q.interrupt() } catch (err) { log('interrupt: ' + err) }
      e.ocupada = false
      enviarRust({ evento: 'conversa', conversa: cmd.conversa, tipo: 'cancelado' })
      break
    }
    case 'fechar': {
      const e = conversas.get(cmd.conversa)
      if (e) { e.fila.fechar(); try { e.q.close() } catch {} }
      break
    }
    case 'parar_agente': {
      const e = conversas.get(cmd.conversa)
      if (e && cmd.tarefa) { try { await e.q.stopTask(String(cmd.tarefa)) } catch (err) { log('stopTask: ' + err) } }
      break
    }
    case 'permissao': {
      const p = pendentes.get(cmd.pedido)
      if (!p) break
      pendentes.delete(cmd.pedido)
      // "lembrar aprovações": devolve a regra que o próprio Claude Code sugeriu (vale até o fim da sessão)
      if (cmd.decisao === 'permitir') p.resolver(cmd.lembrar && Array.isArray(p.sugestoes) && p.sugestoes.length ? { behavior: 'allow', updatedInput: p.entrada, updatedPermissions: p.sugestoes } : { behavior: 'allow', updatedInput: p.entrada })
      else p.resolver({ behavior: 'deny', message: cmd.motivo || 'O Alex negou este pedido.' })
      break
    }
    case 'sair':
      for (const e of conversas.values()) { e.fila.fechar(); try { e.q.close() } catch {} }
      setTimeout(() => process.exit(0), 300)
      break
  }
}

createInterface({ input: process.stdin }).on('line', (linha) => {
  let cmd
  try { cmd = JSON.parse(linha) } catch { return log('linha inválida') }
  tratar(cmd)
}).on('close', () => {
  for (const e of conversas.values()) { try { e.q.close() } catch {} }
  process.exit(0)
})

enviarRust({ evento: 'pronto_para_ouvir', versaoSdk, claude: exeClaude() || '(o do SDK)' })
