// window.ponte — o que as páginas usam para falar com o programa (contrato em docs/ponte.md).
// No programa, a casca injeta este arquivo em todas as janelas antes de qualquer script da página.
// Fora do programa (navegador, testes), vira uma ponte de mentira em memória, para a página não quebrar.
(function () {
  if (window.ponte) return
  const T = () => window.__TAURI__ || null
  const noPrograma = () => !!(T() && T().core && T().core.invoke)

  // ---------- modo navegador: tudo em memória ----------
  const ouvintesFalsos = {}
  const emitirFalso = (nome, dado) => (ouvintesFalsos[nome] || []).forEach((f) => { try { f(dado) } catch (e) { console.error(e) } })
  const falso = {
    raiz: 'A:\\Claude',
    arvore: { '': [{ nome: '03-projetos', tipo: 'pasta' }, { nome: 'LEIA-ME.md', tipo: 'arquivo' }], '03-projetos': [{ nome: 'ponte-de-comando-sara', tipo: 'pasta' }], '03-projetos\\ponte-de-comando-sara': [] },
    estado: { estado: 'esperando', legenda: 'Por aqui', aviso: null, mudo: false, som: false },
    conversas: {},
    config: { tema: 'metal', fosforo: 'ambar', pastaTrabalho: 'A:\\Claude', pastaTrabalhoPadrao: true, miniAoMinimizar: true, toastWindows: 'auto', mudo: false,
      idioma: 'pt-BR', idiomaRespostas: 'seguir', tamanhoLetra: 100, animacoes: 'sistema', padraoNova: { modelo: 'opus', esforco: 'high', rapido: false },
      lembrarAprovacoes: false, somAviso: true, volumeAviso: 70, cantoMini: 'id', abrirComWindows: false, comecarNaBandeja: false, modoConta: 'assinatura' },
  }
  const montarItem = (pai, it) => { const caminho = pai ? pai + '\\' + it.nome : it.nome; return { nome: it.nome, caminho, absoluto: falso.raiz + '\\' + caminho, tipo: it.tipo, link: false, tamanho: null } }
  const relFalso = (d) => {
    let s = String(d || '').replace(/\//g, '\\').replace(/\\+$/, '')
    if (s.toLowerCase().startsWith(falso.raiz.toLowerCase())) s = s.slice(falso.raiz.length).replace(/^\\/, '')
    return s
  }
  const invocarFalso = async (cmd, a = {}) => {
    switch (cmd) {
      case 'pasta_de_trabalho': return { caminho: falso.raiz, nome: falso.raiz.split('\\').pop(), padrao: falso.raiz === 'A:\\Claude', caminhoPadrao: 'A:\\Claude' }
      case 'listar': {
        const p = relFalso(a.pasta)
        if (!(p in falso.arvore)) throw new Error('Essa pasta não existe: ' + p)
        return { pasta: p, absoluto: falso.raiz + (p ? '\\' + p : ''), itens: falso.arvore[p].map((it) => montarItem(p, it)), truncado: false }
      }
      case 'criar_pasta': case 'criar_arquivo': {
        const p = relFalso(a.destino)
        if (/^Biblioteca\\Mem/i.test(p)) throw new Error('A Memória fica fora da pasta de trabalho; criar nela ainda não é permitido.')
        if (!(p in falso.arvore)) throw new Error('Essa pasta não existe.')
        if (falso.arvore[p].some((x) => x.nome.toLowerCase() === String(a.nome).toLowerCase())) throw new Error('Já existe um item com esse nome aqui.')
        const it = { nome: String(a.nome).trim(), tipo: cmd === 'criar_pasta' ? 'pasta' : 'arquivo' }
        falso.arvore[p].push(it)
        if (it.tipo === 'pasta') falso.arvore[p ? p + '\\' + it.nome : it.nome] = []
        return montarItem(p, it)
      }
      case 'trocar_pasta': return null
      case 'voltar_padrao': falso.raiz = 'A:\\Claude'; return invocarFalso('pasta_de_trabalho')
      case 'estado_atual': return falso.estado
      case 'obter_mudo': return falso.estado.mudo
      case 'definir_mudo': falso.estado.mudo = !!a.mudo; emitirFalso('ponte:mudo', falso.estado.mudo); return null
      case 'dispensar_aviso': falso.estado = { ...falso.estado, aviso: null, estado: 'esperando', legenda: 'Por aqui' }; emitirFalso('ponte:estado', falso.estado); return null
      case 'enviar': {
        const p = a.pedido || {}
        setTimeout(() => emitirFalso('ponte:conversa', { conversa: p.conversa, tipo: 'inicio', sessao: 'falsa' }), 50)
        setTimeout(() => emitirFalso('ponte:conversa', { conversa: p.conversa, tipo: 'delta', texto: 'Resposta de ' }), 120)
        setTimeout(() => emitirFalso('ponte:conversa', { conversa: p.conversa, tipo: 'texto', texto: 'Resposta de teste (fora do programa).' }), 200)
        setTimeout(() => emitirFalso('ponte:conversa', { conversa: p.conversa, tipo: 'resultado', texto: 'Resposta de teste (fora do programa).', erro: false }), 260)
        return { conversa: p.conversa, sessao: 'falsa', nova: true, motor: 'falso' }
      }
      case 'mini_canto': return 'inferior-direito'
      case 'config_ler': return Object.assign({}, falso.config)
      case 'config_gravar': {
        const ok = { tema: ['metal', 'fosforo', 'grade'], fosforo: ['ambar', 'verde'], toastWindows: ['auto', 'sempre', 'nunca'] }
        for (const [k, v] of Object.entries(a.parcial || {})) {
          if (!(k in falso.config)) throw new Error('Chave desconhecida: ' + k)
          if (ok[k] && !ok[k].includes(v)) throw new Error('Valor inválido para ' + k)
        }
        Object.assign(falso.config, a.parcial)
        emitirFalso('ponte:config', Object.assign({}, falso.config))
        return Object.assign({}, falso.config)
      }
      case 'tokens_resumo': return { sessoes: [], total: { custo: 0, tokens: 0 }, precosConferidos: false }
      case 'limites': return { disponivel: false }
      default: return null
    }
  }

  const invocar = (cmd, args) => noPrograma() ? T().core.invoke(cmd, args || {}) : invocarFalso(cmd, args || {})
  const ouvir = (nome, fn) => {
    if (noPrograma()) {
      let desligar = null, cancelado = false
      T().event.listen(nome, (e) => fn(e.payload)).then((u) => { if (cancelado) u(); else desligar = u })
      return () => { cancelado = true; if (desligar) desligar() }
    }
    ;(ouvintesFalsos[nome] = ouvintesFalsos[nome] || []).push(fn)
    return () => { ouvintesFalsos[nome] = (ouvintesFalsos[nome] || []).filter((f) => f !== fn) }
  }
  const erroEmTexto = (p) => p.catch((e) => { throw new Error(typeof e === 'string' ? e : (e && e.message) || String(e)) })

  window.ponte = {
    versao: 1,
    get noPrograma() { return noPrograma() },

    // pasta de trabalho
    pastaDeTrabalho: () => invocar('pasta_de_trabalho'),
    trocarPasta: () => erroEmTexto(invocar('trocar_pasta')), // → caminho escolhido (string) ou null
    voltarPadrao: () => invocar('voltar_padrao').then((i) => i && i.caminho),
    listar: (pasta) => erroEmTexto(invocar('listar', { pasta: pasta || '' })),
    criarPasta: (destino, nome) => erroEmTexto(invocar('criar_pasta', { destino: String(destino || ''), nome: String(nome || '') })),
    criarArquivo: (destino, nome) => erroEmTexto(invocar('criar_arquivo', { destino: String(destino || ''), nome: String(nome || '') })),

    // conversa (uma sessão do Claude Code por conversa, com o seu modelo/esforço/velocidade)
    enviar: (p) => erroEmTexto(invocar('enviar', { pedido: {
      conversa: String(p.conversa || ''), texto: String(p.texto || ''),
      modelo: String(p.modelo || ''), esforco: String(p.esforco || ''),
      rapido: typeof p.rapido === 'boolean' ? p.rapido : undefined, velocidade: String(p.velocidade || ''),
      pasta: String(p.pasta || ''),
    } })),
    parar: (conversa) => invocar('parar', { conversa: String(conversa) }),
    responderPermissao: (pedido, permitir, motivo) => erroEmTexto(invocar('responder_permissao', { pedido: String(pedido), permitir: !!permitir, motivo: motivo || null })),
    pararAgente: (conversa, tarefa) => invocar('parar_agente', { conversa: String(conversa), tarefa: String(tarefa) }),
    conversa: (conversa) => invocar('conversa_info', { conversa: String(conversa) }),
    conversasAntigas: (pasta, limite) => invocar('conversas_antigas', { pasta: pasta || null, limite: limite || null }),
    retomarSessao: (conversa, sessao, pasta) => erroEmTexto(invocar('retomar_sessao', { conversa: String(conversa), sessao: String(sessao), pasta: String(pasta || '') })),

    // configurações (gravadas em %APPDATA%\com.pixelmartins.pontedecomando\config.json)
    config: {
      ler: () => invocar('config_ler'),
      gravar: (parcial) => erroEmTexto(invocar('config_gravar', { parcial: parcial || {} })),
    },
    aoConfig: (fn) => ouvir('ponte:config', fn),

    // conta: assinatura (login do Claude Code) ou chave de API (Cofre do Windows)
    conta: {
      status: () => invocar('conta_status'),          // { ok, logado, metodo, email (mascarado), organizacao, plano, modo, temChave }
      modo: () => invocar('conta_modo'),              // 'assinatura' | 'api'
      trocar: () => erroEmTexto(invocar('conta_trocar')),             // login oficial num terminal visível
      sair: (confirmado) => erroEmTexto(invocar('conta_sair', { confirmado: confirmado === true })),
      guardarChave: (chave) => erroEmTexto(invocar('conta_guardar_chave', { chave: String(chave || '') })),
      testarChave: () => invocar('conta_testar_chave'), // { ok, status, erro?, modelos? }
      apagarChave: () => invocar('conta_apagar_chave'),
    },
    aoConta: (fn) => ouvir('ponte:conta', fn),

    // sistema
    sistema: {
      abrirPastaDados: () => erroEmTexto(invocar('sistema_abrir_pasta_dados')),
      exportarConfig: () => erroEmTexto(invocar('sistema_exportar_config')),   // → caminho salvo ou null
      importarConfig: () => erroEmTexto(invocar('sistema_importar_config')),   // → config nova ou null
      restaurarPadrao: (confirmado) => erroEmTexto(invocar('sistema_restaurar_padrao', { confirmado: confirmado === true })),
    },

    // estado da Sara e avisos
    estado: () => invocar('estado_atual'),
    dispensarAviso: () => invocar('dispensar_aviso'),
    mudo: () => invocar('obter_mudo'),
    definirMudo: (mudo) => invocar('definir_mudo', { mudo: !!mudo }),
    mostrarPrincipal: () => invocar('mostrar_principal'),

    // números
    tokens: () => invocar('tokens_resumo'),
    limites: () => invocar('limites'),
    conexoes: () => invocar('conexoes_mcp'),
    info: () => invocar('info_app'),

    // eventos (cada um devolve a função que desliga)
    aoEstado: (fn) => ouvir('ponte:estado', fn),
    aoConversa: (fn) => ouvir('ponte:conversa', fn),
    aoPermissao: (fn) => ouvir('ponte:permissao', fn),
    aoPermissaoFim: (fn) => ouvir('ponte:permissao-fim', fn),
    aoPasta: (fn) => ouvir('ponte:pasta', fn),
    aoMudo: (fn) => ouvir('ponte:mudo', fn),
    aoTokens: (fn) => ouvir('ponte:tokens', fn),

    // uso interno das páginas do programa
    _invocar: invocar,
    _simular: (nome, dado) => { if (!noPrograma()) emitirFalso(nome, dado) },
  }
})()
