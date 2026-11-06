// A tela de CONFIGURAÇÕES da Lia ligada ao programa de verdade (o .exe de debug, isolado e fora da tela):
// fósforo branco + letra atual gravam e voltam ao reabrir, idioma English e volta, e a Conta com o status real
// (`claude auth status`, só leitura). Nada toca a config do Alex.
// Uso: node ferramentas/testes/configuracoes.mjs
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate } from './qa/comum.mjs'

const abrirConfig = async (main) => {
  await main.click('button[aria-controls="cfg-painel"]')
  return ate(() => main.evaluate(() => !!document.getElementById('cfg-painel')), 5000)
}
const botao = (main, texto, depoisDe) => main.evaluate(({ texto, depoisDe }) => {
  const bs = [...document.querySelectorAll('#cfg-painel button')]
  const i0 = depoisDe ? bs.findIndex((b) => b.textContent.trim().toUpperCase().startsWith(depoisDe)) : -1
  const b = bs.find((x, i) => i > i0 && x.textContent.trim().toUpperCase().startsWith(texto))
  if (!b) return false
  b.click(); return true
}, { texto, depoisDe })
const estadoTela = (main) => main.evaluate(() => { const s = window.__ponteMain.logica().state; return { tema: s.tema, fosforo: s.fosforo, letra: s.letra, idioma: s.idioma, html: Object.assign({}, document.documentElement.dataset) } })
const lerArquivo = (app) => JSON.parse(readFileSync(join(app.dados, 'config.json'), 'utf8'))

let salvo = null
{
  const app = await abrir('configuracoes')
  try {
    const main = app.main
    await ate(() => main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 20000)
    conferir('a Configuração abre', await abrirConfig(main))
    // fósforo: o cartão de prévia logo antes de ÂMBAR escolhe o tema; depois a cor e a letra
    await main.evaluate(() => { const bs = [...document.querySelectorAll('#cfg-painel button')]; const i = bs.findIndex((b) => b.textContent.trim().toUpperCase() === 'ÂMBAR'); if (i > 0) bs[i - 1].click() })
    await esperar(200)
    await botao(main, 'BRANCO'); await esperar(200)
    await botao(main, 'ATUAL'); await esperar(600)
    const tela = await estadoTela(main)
    const arq = await ate(() => { const c = lerArquivo(app); return c.tema === 'fosforo' && c.fosforo === 'branco' && c.fosforoLetra === 'atual' && c }, 4000)
    conferir('fósforo branco + letra atual muda na hora na tela', tela.tema === 'fosforo' && tela.fosforo === 'branco' && tela.letra === 'atual', JSON.stringify(tela))
    conferir('e fica gravado no config.json do programa', !!arq, JSON.stringify(lerArquivo(app)).slice(0, 200))
    conferir('a miniatura recebe o tema (config do programa)', await app.mini.evaluate(() => window.ponte.config.ler().then((c) => c.tema === 'fosforo' && c.fosforo === 'branco')))
    // idioma: English e volta
    await main.click('#cfg-aba-idioma'); await esperar(300)
    await botao(main, 'ENGLISH'); await esperar(800)
    const en = await main.evaluate(() => ({ idioma: window.__ponteMain.logica().state.idioma, texto: document.getElementById('cfg-painel').innerText.slice(0, 400) }))
    const enArq = lerArquivo(app).idioma
    conferir('idioma English: a tela troca e o programa grava "en"', en.idioma === 'en' && enArq === 'en' && /LANGUAGE|APPEARANCE|ACCOUNT/i.test(en.texto), `tela=${en.idioma} arquivo=${enArq}`)
    await botao(main, 'PORTUGU'); await esperar(800)
    const pt = await main.evaluate(() => window.__ponteMain.logica().state.idioma)
    conferir('e volta para português', pt === 'pt' && lerArquivo(app).idioma === 'pt-BR', `tela=${pt} arquivo=${lerArquivo(app).idioma}`)
    // conta: status de verdade (claude auth status), e-mail mascarado
    await main.click('#cfg-aba-conta')
    const conta = await ate(() => main.evaluate(() => { const c = window.__ponteMain.logica().state.contaReal; return c && c }), 30000, 500)
    const textoConta = await main.evaluate(() => document.getElementById('cfg-painel').innerText)
    conferir('Conta mostra o status de verdade (e-mail mascarado)', !!conta && conta.ok !== false && (conta.email === '—' || /\*\*\*@/.test(conta.email)) && textoConta.includes(conta.email), conta ? `${conta.email} · ${conta.plano}` : 'sem status')
    // aprovação continua sempre ligada
    const aprov = await main.evaluate(() => window.ponte.config.gravar({ aprovacaoSempre: false }).then(() => 'gravou', (e) => 'recusou: ' + e.message))
    conferir('aprovacaoSempre não desliga pela tela', /recusou|gravou/.test(aprov) && (await main.evaluate(() => window.ponte.config.ler().then((c) => c.aprovacaoSempre))) === true, aprov)
    salvo = readFileSync(join(app.dados, 'config.json'), 'utf8')
  } catch (e) { conferir('roteiro 1 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}

// reabrir com a config que ficou gravada: o tema volta sozinho
{
  const app = await abrir('configuracoes-2', { config: salvo ? JSON.parse(salvo) : undefined, antes: ({ dados, trabalho }) => {} })
  try {
    await ate(() => app.main.evaluate(() => window.__ponteMain && window.__ponteMain.ok), 20000)
    const tela = await ate(() => estadoTela(app.main).then((t) => t.tema === 'fosforo' && t), 8000)
    conferir('ao reabrir, a tela volta em fósforo branco + letra atual', !!tela && tela.fosforo === 'branco' && tela.letra === 'atual', JSON.stringify(tela || (await estadoTela(app.main))))
  } catch (e) { conferir('roteiro 2 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}
process.exit(fimDoPlacar() ? 1 : 0)
