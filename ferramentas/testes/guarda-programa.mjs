// O modo "Controle total com guarda" de ponta a ponta, no .exe de debug isolado, com conversa real (Haiku,
// esforço baixo, 3 turnos curtos): o que é seguro passa sozinho e vai para o registro; o perigoso mostra o
// cartão com a categoria; no modo 'sempre' tudo pede o cartão.
// Uso: node ferramentas/testes/guarda-programa.mjs
import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, ate } from './qa/comum.mjs'

const app = await abrir('guarda-programa')
const C = 'guarda@1'
try {
  const main = app.main
  mkdirSync(join(app.trabalho, 'lixo-teste'), { recursive: true })
  writeFileSync(join(app.trabalho, 'lixo-teste', 'a.txt'), 'a')
  await main.evaluate(() => {
    window.__g = { perm: [], conv: [] }
    window.ponte.aoPermissao((e) => window.__g.perm.push({ ...e, t: Date.now() }))
    window.ponte.aoConversa((e) => window.__g.conv.push({ ...e, t: Date.now() }))
  })
  const cfg = await main.evaluate(() => window.ponte.config.ler())
  conferir('o padrão é aprovacao: "guarda"', cfg.aprovacao === 'guarda', cfg.aprovacao)
  const enviar = (texto) => main.evaluate(({ C, texto }) => window.ponte.enviar({ conversa: C, texto, modelo: 'haiku', esforco: 'low' }), { C, texto })
  const fimDesde = (t) => ate(() => main.evaluate((t) => window.__g.conv.find((e) => e.t >= t && (e.tipo === 'resultado' || e.tipo === 'erro')), t), 120000, 500)
  const cartaoDesde = (t, ms = 90000) => ate(() => main.evaluate((t) => window.__g.perm.find((e) => e.t >= t), t), ms, 300)

  // 1. seguro: escrever na pasta de trabalho passa sozinho
  let t = Date.now()
  await enviar('Use a ferramenta Write para criar o arquivo guarda-ok.txt na pasta atual com o texto oi. Não use nenhuma outra ferramenta.')
  await fimDesde(t)
  const perm1 = await main.evaluate((t) => window.__g.perm.filter((e) => e.t >= t).length, t)
  conferir('modo guarda: Write na pasta de trabalho passa sem cartão', perm1 === 0 && existsSync(join(app.trabalho, 'guarda-ok.txt')), `cartões=${perm1}`)
  const reg = await main.evaluate(() => window.ponte.registro.ler(20))
  conferir('e entra no registro como "liberada pela guarda"', reg.some((r) => r.ferramenta === 'Write' && r.decisao === 'liberada pela guarda'), JSON.stringify(reg.slice(0, 2)))

  // 2. perigoso: rm -rf mostra o cartão com a categoria; negar mantém a pasta
  t = Date.now()
  await enviar('Use a ferramenta Bash para rodar exatamente: rm -rf lixo-teste. Não use nenhuma outra ferramenta.')
  const c2 = await cartaoDesde(t)
  conferir('rm -rf mostra o cartão com a categoria "apagar" e o motivo', !!c2 && c2.categoria === 'apagar' && !!c2.motivo, c2 && `${c2.rotulo} · ${c2.motivo}`)
  if (c2) await main.evaluate((p) => window.ponte.responderPermissao(p, false), c2.pedido)
  await fimDesde(t)
  conferir('negado: a pasta continua lá', existsSync(join(app.trabalho, 'lixo-teste', 'a.txt')))
  const reg2 = await main.evaluate(() => window.ponte.registro.ler(5))
  conferir('o registro guarda "negada no cartão"', reg2.some((r) => r.decisao === 'negada no cartão' && r.categoria === 'apagar'), JSON.stringify(reg2[0]))

  // 3. modo 'sempre': o mesmo Write seguro pede o cartão
  await main.evaluate(() => window.ponte.config.gravar({ aprovacao: 'sempre' }))
  t = Date.now()
  await enviar('Use a ferramenta Write para criar o arquivo sempre.txt na pasta atual com o texto oi. Não use nenhuma outra ferramenta.')
  const c3 = await cartaoDesde(t)
  conferir('modo "sempre": o Write seguro pede o cartão (sem motivo da guarda)', !!c3 && !c3.motivo, c3 && c3.frase)
  if (c3) await main.evaluate((p) => window.ponte.responderPermissao(p, false), c3.pedido)
  await fimDesde(t)
} catch (e) { conferir('o roteiro chegou ao fim', false, e.stack) } finally { await app.fechar() }
process.exit(fimDoPlacar() ? 1 : 0)
