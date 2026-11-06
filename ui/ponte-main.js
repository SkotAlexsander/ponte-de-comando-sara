// Adaptador PROVISÓRIO da Main (estudio/Main.dc.html é a fonte da Sara e não é editada aqui).
// Liga os dados reais na lógica da página (a instância DCLogic do runtime), sem mexer no desenho:
//  1. conversa: ENVIAR/Enter e PARAR viram ponte.enviar/ponte.parar, com o modelo, esforço e
//     velocidade da aba; a resposta aparece ao vivo e as ferramentas viram a lista de atividades;
//  2. árvore de Arquivos: vem de ponte.listar (uma pasta por vez), via `st.nosReais`
//     (o sincronizador aplica `const NOS = st.nosReais || [...]` na cópia da ui);
//  3. arquivo aberto: conteúdo real (só leitura) via `st.conteudoReal`;
//  4. pasta de trabalho: `st.pastaTrabalho` com a pasta gravada;
//  5. placa "SARA · <estado>": segue os eventos do Claude Code (hooks e sessões vivas);
//  6. cartão de permissão em português, por cima da conversa.
// Quando o template passar a chamar a ponte sozinho, cada parte se desliga (ver docs/ponte.md).
(function () {
  if (!/Main\.dc\.html$/i.test(location.pathname)) return
  const CARGA = Math.random().toString(36).slice(2, 8)
  const p = () => window.ponte
  const hora = () => { const d = new Date(); return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0') }
  const OK = '#9BF7D6', TR = '#FFA53A'

  // ---------- achar a lógica da página ----------
  let L = null
  const acharLogica = () => {
    const el = document.getElementById('msg') || document.querySelector('header') || (document.body && document.body.firstElementChild)
    if (!el) return null
    const k = Object.keys(el).find((x) => x.startsWith('__reactFiber'))
    let f = k && el[k]
    while (f && !(f.stateNode && f.stateNode.logic)) f = f.return
    return f ? f.stateNode.logic : null
  }
  const fonteDoTemplate = () => (document.querySelector('script[data-dc-script]') || {}).textContent || ''

  const esperar = () => new Promise((ok) => {
    const t0 = Date.now()
    const tentar = () => { try { L = acharLogica() } catch (e) { L = null } if (L && L.setState) ok(L); else if (Date.now() - t0 < 20000) setTimeout(tentar, 150); else ok(null) }
    tentar()
  })
  const st = () => (L && L.state) || {}
  const set = (patch) => { if (L) L.setState(patch) }

  // ---------- 1. conversa ----------
  const conversas = {} // id da aba → { vivo: índice da mensagem ao vivo, itens: índice da 'ativ' do turno }
  const chave = (id) => id + '@' + CARGA
  const idDaChave = (c) => { const [id, carga] = String(c || '').split('@'); return carga === CARGA ? id : null }
  const lerConfigDaAba = () => {
    const grupo = document.querySelector('[aria-label="Modelo e velocidade desta conversa"]')
    const t = grupo ? grupo.textContent.toUpperCase() : ''
    const modelo = (t.match(/OPUS|SONNET|HAIKU/) || ['OPUS'])[0]
    const esforco = (t.match(/BAIXO|MÉDIO|MEDIO|ALTO|MÁXIMO|MAXIMO/) || ['ALTO'])[0]
    return { modelo, esforco, velocidade: /RÁPIDA|RAPIDA/.test(t) ? 'RÁPIDA' : 'NORMAL' }
  }
  const msgs = (id) => ((st().extra || {})[id] || []).slice()
  const gravarMsgs = (id, lista, extra) => set(Object.assign({ extra: Object.assign({}, st().extra || {}, { [id]: lista }) }, extra || {}))

  async function enviarReal() {
    const s = st()
    const texto = String(s.texto || (document.getElementById('msg') || {}).value || '').trim()
    if (!texto) return
    const id = s.sessao || 'landing'
    if (s.rodandoEm === id) return
    const cfg = lerConfigDaAba()
    const lista = msgs(id).concat([{ t: 'alex', texto, hora: 'ALEX · ' + hora() }])
    const titulos = Object.assign({}, s.titulos || {})
    if (/^nova-/.test(id) && !titulos[id]) titulos[id] = texto.length > 42 ? texto.slice(0, 40) + '…' : texto
    conversas[id] = { vivo: null, itens: null, viuTexto: false }
    gravarMsgs(id, lista, { titulos, texto: '', estado: 'pensando', rodandoEm: id, mic: false })
    try {
      await p().enviar({ conversa: chave(id), texto, modelo: cfg.modelo, esforco: cfg.esforco, velocidade: cfg.velocidade })
    } catch (e) {
      gravarMsgs(id, msgs(id).concat([{ t: 'sara', texto: 'Não consegui começar: ' + e.message, hora: 'SARA · ' + hora() }]), { estado: 'erro', rodandoEm: null })
    }
  }

  function aoConversa(ev) {
    const id = idDaChave(ev.conversa)
    if (!id) return
    const c = conversas[id] || (conversas[id] = { vivo: null, itens: null, viuTexto: false })
    let lista = msgs(id)
    const fecharVivo = () => { c.vivo = null }
    const marcarFeitos = () => {
      if (c.itens == null || !lista[c.itens]) return
      lista[c.itens] = Object.assign({}, lista[c.itens], { itens: lista[c.itens].itens.map((x) => Object.assign({}, x, { marca: '✓', cor: OK })) })
    }
    switch (ev.tipo) {
      case 'inicio': set({ estado: 'pensando' }); return
      case 'delta': {
        if (c.vivo == null || !lista[c.vivo]) { lista.push({ t: 'sara', texto: ev.texto || '', hora: 'SARA · ' + hora() }); c.vivo = lista.length - 1; c.itens = null }
        else lista[c.vivo] = Object.assign({}, lista[c.vivo], { texto: (lista[c.vivo].texto || '') + (ev.texto || '') })
        c.viuTexto = true
        gravarMsgs(id, lista, { estado: 'pensando' })
        return
      }
      case 'texto': {
        if (c.vivo != null && lista[c.vivo]) lista[c.vivo] = Object.assign({}, lista[c.vivo], { texto: ev.texto })
        else { lista.push({ t: 'sara', texto: ev.texto, hora: 'SARA · ' + hora() }); c.itens = null }
        c.viuTexto = true
        fecharVivo()
        gravarMsgs(id, lista)
        return
      }
      case 'ferramenta': {
        fecharVivo()
        const item = { marca: '◆', cor: TR, acao: ev.pai ? 'Funcionário' : (ev.legenda || ev.nome || 'Trabalhando').split(' ')[0], alvo: ev.pai ? (ev.legenda || '') : (ev.legenda || '').split(' ').slice(1).join(' ') || ev.detalhe || '' }
        if (c.itens == null || !lista[c.itens] || lista[c.itens].t !== 'ativ') { lista.push({ t: 'ativ', itens: [] }); c.itens = lista.length - 1 }
        const anteriores = lista[c.itens].itens.map((x) => Object.assign({}, x, { marca: '✓', cor: OK }))
        lista[c.itens] = { t: 'ativ', itens: anteriores.concat([item]).slice(-12) }
        gravarMsgs(id, lista, { estado: /^(Read|Grep|Glob|WebFetch|NotebookRead)$/.test(ev.nome) ? 'lendo' : 'trabalhando' })
        return
      }
      case 'resultado': {
        marcarFeitos(); fecharVivo()
        if (ev.erro) lista.push({ t: 'sara', texto: 'Parei com erro: ' + (ev.texto || ev.subtipo || 'sem detalhe'), hora: 'SARA · ' + hora() })
        else if (!c.viuTexto && ev.texto) lista.push({ t: 'sara', texto: ev.texto, hora: 'SARA · ' + hora() })
        c.itens = null; c.viuTexto = false
        gravarMsgs(id, lista, { estado: ev.erro ? 'erro' : 'pronto', rodandoEm: null })
        setTimeout(() => { if (!st().rodandoEm) set({ estado: 'esperando' }) }, 6000)
        return
      }
      case 'erro': {
        marcarFeitos(); fecharVivo()
        lista.push({ t: 'sara', texto: 'Deu erro: ' + (ev.texto || ''), hora: 'SARA · ' + hora() })
        gravarMsgs(id, lista, { estado: 'erro', rodandoEm: null })
        return
      }
      case 'cancelado': {
        marcarFeitos(); fecharVivo()
        lista.push({ t: 'sara', texto: 'Parei aqui. A conversa continua de onde ficou.', hora: 'SARA · ' + hora() })
        gravarMsgs(id, lista, { estado: 'esperando', rodandoEm: null })
        return
      }
    }
  }

  // ---------- 6. cartão de permissão ----------
  const fila = []
  let cartao = null
  function desenharCartao() {
    if (!cartao) {
      cartao = document.createElement('div')
      cartao.setAttribute('role', 'alertdialog')
      cartao.setAttribute('aria-label', 'Pedido de permissão')
      cartao.style.cssText = 'position:fixed;left:50%;bottom:120px;transform:translateX(-50%);z-index:9999;width:min(560px,calc(100vw - 32px));box-sizing:border-box;padding:16px 18px;background:linear-gradient(180deg,rgba(14,12,6,.98),rgba(8,8,6,.98));border:1px solid rgba(255,197,110,.6);box-shadow:0 10px 30px rgba(0,0,0,.75);color:#E6F1F3;font-family:"IBM Plex Sans",system-ui,sans-serif'
      cartao.innerHTML = '<div data-topo style="font:700 11px Rajdhani,sans-serif;letter-spacing:.22em;color:#FFC56E"></div><div data-frase style="margin-top:8px;font-size:15px;line-height:22px;font-weight:600;word-break:break-word"></div><pre data-detalhe style="margin:8px 0 0;max-height:160px;overflow:auto;padding:8px;background:rgba(0,0,0,.4);font:11px/16px \'JetBrains Mono\',monospace;color:#B8D3D8;white-space:pre-wrap;word-break:break-all"></pre><div data-erro style="margin-top:6px;font-size:12px;color:#FF9A8C"></div><div style="display:flex;gap:8px;margin-top:12px"><button data-sim style="height:34px;padding:0 16px;border:1px solid rgba(155,247,214,.55);background:transparent;color:#9BF7D6;font:700 12px Rajdhani,sans-serif;letter-spacing:.16em;cursor:pointer">PERMITIR</button><button data-nao style="height:34px;padding:0 16px;border:1px solid rgba(255,107,90,.55);background:transparent;color:#FF9A8C;font:700 12px Rajdhani,sans-serif;letter-spacing:.16em;cursor:pointer">NEGAR</button></div>'
      const responder = (permitir) => {
        const atual = fila[0]
        if (!atual) return
        p().responderPermissao(atual.pedido, permitir).catch((e) => { cartao.querySelector('[data-erro]').textContent = e.message })
        fila.shift(); desenharCartao()
      }
      cartao.querySelector('[data-sim]').addEventListener('click', () => responder(true))
      cartao.querySelector('[data-nao]').addEventListener('click', () => responder(false))
      cartao.addEventListener('keydown', (ev) => { if (ev.key === 'Escape') { ev.preventDefault(); responder(false) } })
      document.body.appendChild(cartao)
    }
    const atual = fila[0]
    cartao.hidden = !atual
    cartao.style.display = atual ? 'block' : 'none'
    if (!atual) return
    cartao.querySelector('[data-topo]').textContent = 'A SARA PRECISA DA SUA PERMISSÃO' + (fila.length > 1 ? ` · 1 DE ${fila.length}` : '')
    cartao.querySelector('[data-frase]').textContent = atual.frase
    cartao.querySelector('[data-detalhe]').textContent = atual.detalhe || ''
    cartao.querySelector('[data-erro]').textContent = ''
    cartao.querySelector('[data-nao]').focus() // o foco começa no lado seguro
  }

  // ---------- 2 e 3. árvore real e arquivo aberto ----------
  const nos = {} // id → { id, nome, pasta, nivel, pai, caminho, carregado, filhos }
  let raizAbs = ''
  const idDe = (caminho) => caminho ? 'r:' + caminho : 'raiz'
  const usaArvoreReal = () => /st\.nosReais/.test(fonteDoTemplate()) || window.__ponteRemendoArvore === true
  function listaPlana() {
    const saida = []
    const andar = (id) => { const n = nos[id]; if (!n) return; saida.push({ id: n.id, nome: n.nome, pasta: n.pasta, nivel: n.nivel, pai: n.pai }); (n.filhos || []).forEach(andar) }
    andar('raiz')
    return saida
  }
  async function carregar(id) {
    const n = nos[id]
    if (!n || !n.pasta || n.carregado || n.carregando) return
    n.carregando = true
    try {
      const r = await p().listar(n.caminho)
      n.filhos = r.itens.map((it) => {
        const fid = idDe(it.caminho)
        nos[fid] = Object.assign(nos[fid] || {}, { id: fid, nome: it.nome + (it.link ? ' ↗' : ''), pasta: it.tipo === 'pasta', nivel: n.nivel + 1, pai: id, caminho: it.caminho })
        return fid
      })
      if (r.truncado) n.filhos.push(idDe(n.caminho + '\\…'))
      n.carregado = true
    } catch (e) {
      n.filhos = []; n.carregado = true
      console.warn('ponte: não listei', n.caminho, e.message)
    } finally { n.carregando = false }
    set({ nosReais: listaPlana() })
  }
  async function montarRaiz() {
    const info = await p().pastaDeTrabalho()
    raizAbs = info.caminho
    Object.keys(nos).forEach((k) => delete nos[k])
    nos.raiz = { id: 'raiz', nome: info.nome || raizAbs, pasta: true, nivel: 0, pai: null, caminho: '', carregado: false, filhos: [] }
    set({ nosReais: listaPlana(), aberto: 'raiz', alvo: 'raiz', abertas: { raiz: true }, novos: [], pastaTrabalho: info.caminho })
    await carregar('raiz')
  }
  let ultimoAberto = null
  async function depoisDeMudar() {
    const s = st()
    if (usaArvoreReal() && s.nosReais) {
      const abertas = s.abertas || {}
      Object.keys(abertas).filter((k) => abertas[k] && nos[k] && !nos[k].carregado).forEach(carregar)
      // item criado pela tela: relê a pasta-mãe e tira o provisório
      if ((s.novos || []).length) {
        const pais = [...new Set(s.novos.map((x) => x.pai))]
        set({ novos: [] })
        for (const pai of pais) if (nos[pai]) { nos[pai].carregado = false; await carregar(pai) }
      }
      // arquivo aberto: conteúdo real, só leitura
      if (s.aberto && s.aberto !== ultimoAberto && nos[s.aberto] && !nos[s.aberto].pasta) {
        ultimoAberto = s.aberto
        const alvo = s.aberto
        p()._invocar('ler_arquivo', { caminho: nos[alvo].caminho }).then((texto) => {
          set({ conteudoReal: Object.assign({}, st().conteudoReal || {}, { [alvo]: String(texto).split(/\r?\n/).slice(0, 400) }) })
        }).catch((e) => set({ conteudoReal: Object.assign({}, st().conteudoReal || {}, { [alvo]: ['(' + e.message + ')'] }) }))
      }
    }
  }

  // ---------- 5. placa da Sara pelos eventos de fora ----------
  // se a Main tiver o estado "precisa" (placa PRECISO DE VOCÊ), usa ele; senão, o mais perto é "escutando"
  const temPrecisa = () => /\bprecisa\s*:\s*\{/.test(fonteDoTemplate())
  const MAPA = { esperando: 'esperando', trabalhando: 'trabalhando', precisa: 'escutando', pronto: 'pronto', erro: 'erro' }
  const mapear = (k) => (k === 'precisa' && temPrecisa() ? 'precisa' : MAPA[k])
  // som curto (o mesmo da miniatura): com a Main na tela, o aviso é dela
  let ctxSom = null
  let somCfg = { somAviso: true, volumeAviso: 70 }
  function tocar(tipo) {
    if (!somCfg.somAviso || !somCfg.volumeAviso) return
    try {
      ctxSom = ctxSom || new (window.AudioContext || window.webkitAudioContext)()
      if (ctxSom.state === 'suspended') ctxSom.resume()
      const notas = tipo === 'erro' ? [523, 392] : tipo === 'precisa' ? [659, 880] : [784, 1175]
      notas.forEach((f, i) => {
        const o = ctxSom.createOscillator(), g = ctxSom.createGain(), t = ctxSom.currentTime + i * 0.14
        o.type = 'sine'; o.frequency.value = f
        g.gain.setValueAtTime(0.0001, t); g.gain.exponentialRampToValueAtTime(Math.max(0.0002, 0.16 * somCfg.volumeAviso / 100), t + 0.02); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.45)
        o.connect(g).connect(ctxSom.destination); o.start(t); o.stop(t + 0.5)
      })
      window.__ultimoSomMain = { tipo, quando: Date.now(), contexto: ctxSom.state }
    } catch (err) { window.__ultimoSomMain = { erro: String(err) } }
  }
  function aoEstado(e) {
    if (e.som && !e.mudo && e.aviso && e.onde === 'principal') tocar(e.aviso.tipo)
    if (st().rodandoEm) return // a conversa daqui manda na placa enquanto roda
    const k = mapear(e.estado)
    if (k && st().estado !== k) set({ estado: k })
  }

  // ---------- 7. consumo (motor de tokens; custo sempre estimado) ----------
  const k = (n) => n >= 1e6 ? (n / 1e6).toFixed(1).replace('.', ',') + 'M' : n >= 1000 ? Math.round(n / 1000) + 'k' : String(n || 0)
  const dinheiro = (v) => 'US$ ' + (v || 0).toFixed(2).replace('.', ',')
  const JANELA = 200000 // contexto de referência para a barra (o tamanho real depende do modelo)
  let sessaoDaAba = null
  async function atualizarConsumo(r) {
    if (!r || !Array.isArray(r.sessoes) || !L) return
    const id = st().sessao || 'landing'
    try { const c = await p().conversa(chave(id)); sessaoDaAba = c && c.sessao } catch {}
    const medidas = r.sessoes.filter((x) => x.respostas)
    const s = medidas.find((x) => x.sessao === sessaoDaAba) || medidas.slice().sort((a, b) => (b.atualizadaEm || 0) - (a.atualizadaEm || 0))[0]
    const caixa = document.querySelector('section[aria-label="Consumo de tokens"]')
    if (!s || !caixa) return
    const maior = Math.max(1, ...s.agentes.map((a) => a.tokens || 0))
    set({
      consumoReal: s.agentes.slice(0, 4).map((a) => ({ nome: String(a.nome).slice(0, 22), cargo: a.tipo || '', tokens: k(a.tokens), largura: Math.round(100 * (a.tokens || 0) / maior) + '%', cor: a.estado === 'rodando' ? '#FFA53A' : '#9BF7D6' })),
      skillsReais: s.skills.slice(0, 3).map((x) => ({ nome: x.nome + (x.vezes > 1 ? ' ×' + x.vezes : '') })),
    })
    // os números do topo do painel são texto fixo no protótipo: troca só o texto
    const spans = caixa.querySelectorAll('span')
    const pct = Math.min(100, Math.round(100 * (s.contexto || 0) / JANELA))
    for (const el of spans) {
      if (el.textContent === 'exemplo' || el.dataset.ponte === 'rotulo') { el.dataset.ponte = 'rotulo'; el.textContent = 'estimado'; el.title = (s.nome || '') + ' · preços não conferidos com a Anthropic' }
      else if (/^\d+%$/.test(el.textContent) || el.dataset.ponte === 'pct') { el.dataset.ponte = 'pct'; el.textContent = pct + '%'; el.title = k(s.contexto) + ' de ' + k(JANELA) + ' (referência)' }
      else if (el.parentElement && el.parentElement.classList.contains('trilho-prog')) el.style.width = pct + '%'
      else if (/entrada ·/.test(el.textContent) || el.dataset.ponte === 'linha') {
        el.dataset.ponte = 'linha'
        el.textContent = `${k(s.entrada + s.leituraCache + s.escritaCache)} entrada · ${k(s.saida)} saída · `
        const din = document.createElement('span'); din.style.color = '#FFA53A'; din.textContent = dinheiro(s.custo); el.appendChild(din)
        el.title = `contexto agora ${k(s.contexto)} · total processado ${k(s.totalProcessado)} · material novo ${k(s.materialNovo)}` + (s.cache ? ` · cache de ${s.cache.ttlMinutos} min${s.cache.suposto ? ' (suposto)' : ''}` : '')
      }
    }
  }

  // ---------- ligar tudo ----------
  esperar().then(async (logica) => {
    if (!logica || !window.ponte) { console.warn('ponte: não achei a lógica da Main; ficou só o protótipo'); return }
    const fonte = fonteDoTemplate()
    const assumirConversa = !/ponte\.enviar/.test(fonte)
    // envolve o setState para perceber pastas abertas e itens criados
    const original = logica.setState.bind(logica)
    logica.setState = (patch, cb) => { const r = original(patch, cb); setTimeout(depoisDeMudar, 0); return r }

    if (assumirConversa) {
      window.addEventListener('keydown', (ev) => {
        if (ev.target && ev.target.id === 'msg' && ev.key === 'Enter' && !ev.isComposing && !ev.shiftKey) { ev.preventDefault(); ev.stopImmediatePropagation(); enviarReal() }
      }, true)
      window.addEventListener('click', (ev) => {
        const b = ev.target && ev.target.closest && ev.target.closest('button')
        if (!b) return
        if (b.classList.contains('ambar') && /ENVIAR/i.test(b.textContent)) { ev.preventDefault(); ev.stopImmediatePropagation(); enviarReal() }
        else if (b.classList.contains('parar')) { ev.preventDefault(); ev.stopImmediatePropagation(); const id = st().rodandoEm; if (id) p().parar(chave(id)) }
      }, true)
      p().aoConversa(aoConversa)
    }
    p().aoPermissao((pedido) => { fila.push(pedido); desenharCartao() })
    p().aoPermissaoFim((f) => { const i = fila.findIndex((x) => x.pedido === f.pedido); if (i >= 0) { fila.splice(i, 1); desenharCartao() } })
    p().aoEstado(aoEstado)
    p().aoPasta(() => montarRaiz())
    if (usaArvoreReal()) await montarRaiz()
    else { const i = await p().pastaDeTrabalho(); set({ pastaTrabalho: i.caminho }) }
    // tema de display: a Main da Sara põe o data-tema na área da conversa; aqui fica também no <html>
    const aplicarTema = (c) => { if (c) { somCfg = { somAviso: c.somAviso !== false, volumeAviso: typeof c.volumeAviso === 'number' ? c.volumeAviso : 70 }; document.documentElement.dataset.tema = c.tema || 'metal'; document.documentElement.dataset.fosforo = c.fosforo || 'ambar' } }
    p().config.ler().then(aplicarTema).catch(() => {})
    p().aoConfig(aplicarTema)
    p().aoTokens(atualizarConsumo)
    p().tokens().then(atualizarConsumo)
    setInterval(() => p().tokens().then(atualizarConsumo), 15000)
    window.__ponteMain = { ok: true, assumirConversa, arvoreReal: usaArvoreReal(), carga: CARGA, logica: () => L }
  })
})()
