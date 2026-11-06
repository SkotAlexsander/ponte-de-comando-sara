// QA · conversa real pelo SDK (Haiku, esforço baixo; 4 turnos curtos numa sessão só):
// mandar com a conversa respondendo, PARAR, permissão aprovada, sidecar morto com o cartão aberto, retomar.
// Uso: node ferramentas/testes/qa/conversa.mjs
import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate, filhos, vivo, TASKKILL } from './comum.mjs'

const app = await abrir('conversa')
const C = 'qa@1'
const evs = () => app.main.evaluate(() => window.__qa)
try {
  await app.main.evaluate(() => {
    window.__qa = { conv: [], perm: [], fim: [], est: [] }
    const q = window.__qa, P = window.ponte
    P.aoConversa((e) => q.conv.push({ ...e, t: Date.now() }))
    P.aoPermissao((e) => q.perm.push({ ...e, t: Date.now() }))
    P.aoPermissaoFim((e) => q.fim.push({ ...e, t: Date.now() }))
    P.aoEstado((e) => q.est.push({ estado: e.estado, aviso: e.aviso && e.aviso.titulo, t: Date.now() }))
  })
  const enviar = (texto) => app.main.evaluate(({ C, texto }) => window.ponte.enviar({ conversa: C, texto, modelo: 'haiku', esforco: 'low' }).then((r) => ({ ok: true, r }), (e) => ({ ok: false, e: e.message })), { C, texto })
  const desde = async (t, tipo) => (await evs()).conv.filter((e) => e.t >= t && (!tipo || e.tipo === tipo))

  // ---------- turno 1: mandar de novo com a conversa respondendo, e PARAR ----------
  let t = Date.now()
  const a = await enviar('Conte de 1 a 120, um número por linha, sem mais nada.')
  const b = await enviar('Outra coisa.')
  conferir('mandar com a conversa ainda respondendo é recusado com mensagem clara', a.ok && !b.ok && /trabalhando/.test(b.e), b.e)
  await ate(async () => (await desde(t, 'delta')).length > 0, 60000, 200)
  const tParar = Date.now()
  await app.main.evaluate((C) => window.ponte.parar(C), C)
  await esperar(9000)
  const depoisParar = await desde(tParar)
  const est1 = await app.main.evaluate(() => window.ponte.estado())
  conferir('PARAR emite "cancelado"', depoisParar.some((e) => e.tipo === 'cancelado'), depoisParar.map((e) => e.tipo).filter((x) => x !== 'delta').join(','))
  conferir('depois do PARAR a Sara não diz "travei" (estado não vira erro)', est1.estado !== 'erro', `estado=${est1.estado} · aviso=${est1.aviso && est1.aviso.titulo} · ${est1.aviso && est1.aviso.texto}`)

  // ---------- turno 2: permissão APROVADA ----------
  t = Date.now()
  const r2 = await enviar('Use a ferramenta Write para criar o arquivo aprovado.txt na pasta atual com o texto oi. Não use nenhuma outra ferramenta.')
  conferir('a conversa parada aceita mensagem nova', r2.ok, r2.e || '')
  const p2 = await ate(async () => (await evs()).perm.find((x) => x.t >= t), 90000, 300)
  conferir('cartão de permissão chegou (Write)', !!p2 && /Escrever o arquivo/.test(p2.frase), p2 && p2.frase)
  if (p2) await app.main.evaluate((pedido) => window.ponte.responderPermissao(pedido, true), p2.pedido)
  const fim2 = await ate(async () => (await desde(t, 'resultado'))[0], 90000, 300)
  conferir('permitir cria o arquivo e a conversa termina sem erro', !!fim2 && !fim2.erro && existsSync(join(app.trabalho, 'aprovado.txt')), fim2 && String(fim2.texto).slice(0, 80))
  const est2 = await app.main.evaluate(() => window.ponte.estado())
  conferir('depois de aprovar, a Sara fica em "pronto"', est2.estado === 'pronto', `estado=${est2.estado}`)

  // ---------- turno 3: sidecar morre com o cartão aberto ----------
  t = Date.now()
  await enviar('Use a ferramenta Write para criar o arquivo morto.txt na pasta atual com o texto oi. Não use nenhuma outra ferramenta.')
  const p3 = await ate(async () => (await evs()).perm.find((x) => x.t >= t), 90000, 300)
  const node = filhos(app.prog.pid).find((f) => /node/i.test(f.nome))
  const claude = node && filhos(node.pid).find((f) => /claude/i.test(f.nome))
  conferir('cartão aberto e sidecar achado', !!p3 && !!node && !!claude, `node=${node && node.pid} claude=${claude && claude.pid}`)
  const tMorte = Date.now()
  if (node) spawnSync(TASKKILL, ['/PID', String(node.pid), '/F'], { windowsHide: true }) // só o node, como numa queda
  await esperar(6000)
  const q3 = await evs()
  conferir('sidecar morto: o cartão some (permissao-fim)', p3 && q3.fim.some((f) => f.pedido === p3.pedido && f.t >= tMorte))
  conferir('sidecar morto: a conversa recebe o erro "O processo da conversa fechou"', q3.conv.some((e) => e.t >= tMorte && e.tipo === 'erro'))
  const est3 = await app.main.evaluate(() => window.ponte.estado())
  conferir('sidecar morto: a Sara sai de "precisa de você"/"trabalhando"', !['precisa', 'trabalhando'].includes(est3.estado), `estado=${est3.estado} · legenda=${est3.legenda} · aviso=${est3.aviso && est3.aviso.titulo}`)
  conferir('sidecar morto: o claude.exe que ele abriu não fica órfão rodando', !(claude && vivo(claude.pid)), claude && vivo(claude.pid) ? `claude.exe ${claude.pid} ainda vivo ${((Date.now() - tMorte) / 1000).toFixed(0)} s depois` : '')
  const resp3 = await app.main.evaluate((p) => window.ponte.responderPermissao(p, true).then(() => 'aceitou', (e) => e.message), p3 && p3.pedido)
  conferir('responder o cartão velho depois da queda dá mensagem clara', /respondido|cancelado|não está/.test(resp3), resp3)
  conferir('o arquivo do pedido morto não foi criado', !existsSync(join(app.trabalho, 'morto.txt')))

  // ---------- turno 4: retomar depois da queda ----------
  t = Date.now()
  const r4 = await enviar('Na minha mensagem anterior a esta, que nome de arquivo eu pedi? Responda só o nome do arquivo.')
  const fim4 = await ate(async () => (await desde(t, 'resultado'))[0], 120000, 300)
  const texto4 = fim4 ? String(fim4.texto) : ''
  conferir('a próxima mensagem abre outro sidecar e RETOMA a mesma conversa (lembra do pedido)', r4.ok && !!fim4 && /morto\.txt/i.test(texto4), `${r4.ok ? '' : r4.e} resposta: ${texto4.slice(0, 80)}`)
  const conv = await app.main.evaluate((C) => window.ponte.conversa(C), C)
  conferir('a sessão é a mesma do começo', !!conv && conv.iniciada, conv && conv.sessao)
} catch (e) {
  conferir('o roteiro chegou ao fim', false, e.stack)
} finally {
  await app.fechar()
}
process.exit(fimDoPlacar() ? 1 : 0)
