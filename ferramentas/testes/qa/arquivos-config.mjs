// QA · config.json corrompido, pasta de trabalho inexistente ou sem permissão, junção para fora,
// nomes com acento/emoji/reservados e as barreiras de caminho dos comandos da ponte.
// Uso: node ferramentas/testes/qa/arquivos-config.mjs
import { spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync, readFileSync, existsSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { abrir, conferir, fimDoPlacar, esperar, ate } from './comum.mjs'

// ---------- 1. config.json com UM campo de tipo errado ----------
{
  let trabalhoGuardado
  const app = await abrir('config-tipo', {
    config: null,
    antes: ({ dados, trabalho }) => {
      trabalhoGuardado = trabalho
      writeFileSync(join(dados, 'config.json'), JSON.stringify({ pastaDeTrabalho: trabalho, tema: 'grade', miniCanto: 'superior-esquerdo', mudo: 'sim' }))
    },
  })
  try {
    const p = await app.main.evaluate(() => window.ponte.pastaDeTrabalho())
    const c = await app.main.evaluate(() => window.ponte.config.ler())
    conferir('config com um campo de tipo errado ("mudo": "sim") mantém os outros campos', p.caminho.toLowerCase() === trabalhoGuardado.toLowerCase() && c.tema === 'grade', `pasta=${p.caminho} · tema=${c.tema}`)
    await app.main.evaluate(() => window.ponte.definirMudo(false))
    await esperar(300)
    const depois = JSON.parse(readFileSync(join(app.dados, 'config.json'), 'utf8'))
    const copia = readdirSync(app.dados).filter((f) => /config.*(bak|corromp|velh|ruim)/i.test(f))
    conferir('a config que não leu não é sobrescrita sem cópia', depois.pastaDeTrabalho === trabalhoGuardado || copia.length > 0, `depois de um clique em "silenciar": pastaDeTrabalho=${depois.pastaDeTrabalho} · tema=${depois.tema} · miniCanto=${depois.miniCanto} · cópia=${copia.join(',') || 'nenhuma'}`)
  } catch (e) { conferir('roteiro 1 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}

// ---------- 2. config.json que não é JSON ----------
{
  const app = await abrir('config-lixo', { config: '{"pastaDeTrabalho": "C:\\\\x", tema' })
  try {
    const ok = app.saiu === null && (await app.main.evaluate(() => window.ponte.info().then((i) => !!i.versao)))
    conferir('config.json que não é JSON: o programa abre mesmo assim', ok)
  } catch (e) { conferir('roteiro 2 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}

// ---------- 3. pasta de trabalho gravada que não existe mais ----------
{
  const app = await abrir('pasta-sumiu', { config: { pastaDeTrabalho: join(tmpdir(), 'ponte-qa-nao-existe-' + Date.now()) } })
  try {
    const p = await app.main.evaluate(() => window.ponte.pastaDeTrabalho())
    const tela = await app.main.evaluate(() => document.body.innerText)
    conferir('pasta gravada sumiu: o Alex é avisado (e não cai calado na padrão)', /não existe|sumiu|não achei/i.test(tela), `caiu em ${p.caminho} (padrão=${p.padrao}) sem aviso na tela`)
  } catch (e) { conferir('roteiro 3 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}

// ---------- 4. barreiras, nomes, junção, sem permissão ----------
{
  const fora = join(tmpdir(), 'ponte-qa-fora-da-pasta')
  rmSync(fora, { recursive: true, force: true }); mkdirSync(fora, { recursive: true })
  writeFileSync(join(fora, 'segredo.txt'), 'SEGREDO DE FORA')
  const app = await abrir('barreiras', {
    antes: ({ trabalho }) => {
      spawnSync('C:\\Windows\\System32\\cmd.exe', ['/c', 'mklink', '/J', join(trabalho, 'atalho'), fora], { windowsHide: true })
      mkdirSync(join(trabalho, 'trancada'))
      writeFileSync(join(trabalho, 'trancada', 'x.txt'), 'x')
      const usuario = process.env.USERDOMAIN + '\\' + process.env.USERNAME
      spawnSync('C:\\Windows\\System32\\icacls.exe', [join(trabalho, 'trancada'), '/deny', `${usuario}:(OI)(CI)(RX)`], { windowsHide: true })
    },
  })
  try {
    const r = await app.main.evaluate(async (t) => {
      const tenta = (pr) => pr.then((v) => ({ ok: true, v }), (e) => ({ ok: false, e: e.message }))
      const P = window.ponte
      return {
        acento: await tenta(P.criarPasta('', 'Ação e Coração')),
        emoji: await tenta(P.criarArquivo('', 'notas 🎉✨.md')),
        emojiLista: await tenta(P.listar('')),
        com1sobrescrito: await tenta(P.criarArquivo('', 'COM¹.txt')),
        conout: await tenta(P.criarArquivo('', 'CONOUT$')),
        nulEspaco: await tenta(P.criarArquivo('', 'NUL .txt')),
        juncaoLista: await tenta(P.listar('atalho')),
        juncaoLer: await tenta(P._invocar('ler_arquivo', { caminho: 'atalho\\segredo.txt' })),
        juncaoCriar: await tenta(P.criarArquivo('atalho', 'x.txt')),
        trancadaLista: await tenta(P.listar('trancada')),
        trancadaCriar: await tenta(P.criarArquivo('trancada', 'y.txt')),
        ads: await tenta(P._invocar('ler_arquivo', { caminho: 'projeto-a\\LEIA-ME.md::$DATA' })),
        unc: await tenta(P.listar('\\\\127.0.0.1\\C$')),
        dispositivo: await tenta(P._invocar('ler_arquivo', { caminho: '\\\\.\\C:\\Windows\\win.ini' })),
      }
    }, app.trabalho)
    conferir('nome com acento cria', r.acento.ok, JSON.stringify(r.acento))
    conferir('nome com emoji cria e aparece na lista', r.emoji.ok && r.emojiLista.ok && r.emojiLista.v.itens.some((i) => i.nome === 'notas 🎉✨.md'), JSON.stringify(r.emoji))
    conferir('"COM¹.txt" (reservado no Windows) é recusado pela validação', !r.com1sobrescrito.ok, JSON.stringify(r.com1sobrescrito))
    conferir('"CONOUT$" (reservado) é recusado pela validação', !r.conout.ok, JSON.stringify(r.conout))
    conferir('"NUL .txt" (reservado) é recusado', !r.nulEspaco.ok, JSON.stringify(r.nulEspaco))
    conferir('junção para fora: listar, ler e criar são recusados', !r.juncaoLista.ok && !r.juncaoLer.ok && !r.juncaoCriar.ok && !existsSync(join(fora, 'x.txt')), [r.juncaoLista.e, r.juncaoLer.e, r.juncaoCriar.e].join(' / '))
    conferir('pasta sem permissão: a mensagem diz "sem permissão" (não "não existe")', !r.trancadaLista.ok && /permiss|acesso negado/i.test(r.trancadaLista.e), JSON.stringify(r.trancadaLista))
    conferir('caminho de rede (\\\\127.0.0.1\\C$) é recusado', !r.unc.ok, JSON.stringify(r.unc).slice(0, 160))
    conferir('caminho de dispositivo (\\\\.\\C:) é recusado', !r.dispositivo.ok, JSON.stringify(r.dispositivo).slice(0, 160))

    // comandos que aceitam caminho de FORA da pasta de trabalho
    // (06/11, Davi: recusar agora REJEITA a promessa com a mensagem; rejeitar conta como recusa)
    const alheio = await app.main.evaluate(() => window.__TAURI__.core.invoke('conversas_antigas', { pasta: 'A:\\Claude', limite: 3 }).catch(() => null))
    conferir('conversas_antigas recusa pasta fora da pasta de trabalho', !alheio || !alheio.conversas || alheio.conversas.length === 0, `devolveu ${alheio && alheio.conversas ? alheio.conversas.length : 0} conversa(s) de A:\\Claude (com nome = último pedido), estando a pasta de trabalho em %TEMP%`)
    const barreira = await app.main.evaluate(() => window.ponte.config.gravar({ pastaTrabalho: 'C:\\Windows' }).then((c) => c.pastaTrabalho, (e) => 'recusou: ' + e.message))
    conferir('a própria página não consegue mudar a barreira para C:\\Windows sem o seletor do Windows', /recusou/.test(barreira), barreira)
    await app.main.evaluate((t) => window.ponte.config.gravar({ pastaTrabalho: t }).catch(() => null), app.trabalho) // (06/11: recusado de propósito, a pasta nem mudou)
    const relativa = await app.main.evaluate(() => window.ponte.config.gravar({ pastaTrabalho: '.' }).then((c) => c.pastaTrabalho, (e) => 'recusou: ' + e.message))
    conferir('pastaTrabalho "." é recusada (seria relativa à pasta do processo)', /recusou/.test(relativa), relativa)
    await app.main.evaluate((t) => window.ponte.config.gravar({ pastaTrabalho: t }).catch(() => null), app.trabalho) // (06/11: recusado de propósito, a pasta nem mudou)
    const morto = await app.main.evaluate(() => window.__TAURI__.core.invoke('mini_soltou').then(() => 'respondeu', (e) => 'recusou: ' + e))
    conferir('comando morto mini_soltou não está exposto', /recusou/.test(morto), morto)
  } catch (e) { conferir('roteiro 4 chegou ao fim', false, e.stack) } finally {
    const usuario = process.env.USERDOMAIN + '\\' + process.env.USERNAME
    spawnSync('C:\\Windows\\System32\\icacls.exe', [join(app.trabalho, 'trancada'), '/remove:d', usuario], { windowsHide: true })
    spawnSync('C:\\Windows\\System32\\cmd.exe', ['/c', 'rmdir', join(app.trabalho, 'atalho')], { windowsHide: true })
    await app.fechar()
    rmSync(fora, { recursive: true, force: true })
  }
}

// ---------- 5. miniatura com monitor e canto que não existem ----------
{
  const app = await abrir('mini-monitor', { config: null, antes: ({ dados, trabalho }) => writeFileSync(join(dados, 'config.json'), JSON.stringify({ pastaDeTrabalho: trabalho, miniCanto: 'meio-da-tela', miniMonitor: '\\\\.\\DISPLAY99' })) })
  try {
    await app.main.evaluate(() => window.__TAURI__.window.getCurrentWindow().minimize())
    const vis = await ate(() => app.main.evaluate(() => window.ponte.info().then((i) => i.janelas.mini)), 5000)
    const canto = await app.mini.evaluate(() => window.ponte._invocar('mini_canto'))
    conferir('monitor gravado que não existe e canto inválido: a miniatura abre no canto padrão do monitor principal', !!vis && canto === 'inferior-direito' && app.saiu === null, `visível=${vis} canto=${canto}`)
  } catch (e) { conferir('roteiro 5 chegou ao fim', false, e.stack) } finally { await app.fechar() }
}

process.exit(fimDoPlacar() ? 1 : 0)
