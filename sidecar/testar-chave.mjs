// Testa a chave de API da Anthropic com a chamada mais barata que existe: a lista de modelos (não gera texto).
// A chave chega SÓ pela variável de ambiente ANTHROPIC_API_KEY deste processo (a casca lê do Cofre do Windows).
// Escreve no stdout um JSON { ok, status, erro?, modelos? } e NUNCA a chave.
const chave = process.env.ANTHROPIC_API_KEY || ''
let respondeu = false
const responder = (o) => { if (respondeu) return; respondeu = true; process.stdout.write(JSON.stringify(o)); process.exitCode = 0 }
if (!chave) { responder({ ok: false, erro: 'sem chave' }) } else {
const parar = new AbortController()
setTimeout(() => parar.abort(), 15000).unref()
try {
  const r = await fetch('https://api.anthropic.com/v1/models?limit=5', {
    headers: { 'x-api-key': chave, 'anthropic-version': '2023-06-01' },
    signal: parar.signal,
  })
  const corpo = await r.json().catch(() => ({}))
  if (r.ok) responder({ ok: true, status: r.status, modelos: (corpo.data || []).map((m) => m.id).slice(0, 5) })
  // a mensagem de erro da API não contém a chave; mesmo assim, corta qualquer pedaço dela por garantia
  const msg = String((corpo.error && corpo.error.message) || r.statusText || 'erro').split(chave).join('[chave]')
  responder({ ok: false, status: r.status, erro: msg.slice(0, 200) })
} catch (e) {
  responder({ ok: false, erro: e.name === 'AbortError' ? 'sem resposta em 15 s' : String(e.message || e).slice(0, 200) })
}
}
