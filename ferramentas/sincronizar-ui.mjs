// Recopia o protótipo da Sara para dentro do programa.
// Origem (nunca editada daqui): A:\Claude\00-empresa\corpos\sara\interface\estudio-local\
// Destino: ui/estudio/ (é só cópia; tudo nela é substituído a cada sincronização).
// Os arquivos do programa (mini.html, carregamento.html, ponte.js, ponte-main.js) ficam em ui/ e não são tocados.
//
// Uso: node ferramentas/sincronizar-ui.mjs [pasta-de-origem]
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join, relative, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const origem = resolve(process.argv[2] || process.env.PONTE_UI_ORIGEM || 'A:/Claude/00-empresa/corpos/sara/interface/estudio-local')
const destino = join(raiz, 'ui', 'estudio')

// Precisa existir para o programa funcionar.
const OBRIGATORIOS = ['Main.dc.html', 'Fluxo.dc.html', 'support.js', 'vendor/react.js', 'vendor/react-dom.js', 'assets']
// O que não vai para o programa (versões antigas e o README do exportador).
const PULAR = new Set(['README.md', 'Main-v7.dc.html', 'Fluxo-kit-metal.dc.html'])

if (!existsSync(origem)) {
  console.error(`Origem não encontrada: ${origem}`)
  process.exit(1)
}
const faltando = OBRIGATORIOS.filter((f) => !existsSync(join(origem, f)))
if (faltando.length) {
  console.error(`A origem não tem: ${faltando.join(', ')}. Nada foi copiado.`)
  process.exit(1)
}

// destino é uma pasta gerada; garantir que é a nossa antes de limpar
if (!destino.startsWith(join(raiz, 'ui'))) throw new Error('destino fora de ui/')
rmSync(destino, { recursive: true, force: true })
mkdirSync(destino, { recursive: true })

// Remendos na CÓPIA (nunca na origem): ligam os dados reais que o adaptador ui/ponte-main.js entrega.
// Cada um só entra se a fonte ainda não fizer isso sozinha, e precisa casar exatamente uma vez.
const REMENDOS = [
  { arquivo: 'Main.dc.html', jaTem: 'st.nosReais', de: 'const NOS = [', para: 'const NOS = (st.nosReais && st.nosReais.slice()) || [', motivo: 'árvore de Arquivos real (ponte.listar)' },
  { arquivo: 'Main.dc.html', jaTem: 'st.conteudoReal', de: 'const CONTEUDO = {', para: 'const CONTEUDO = st.conteudoReal || {', motivo: 'conteúdo real do arquivo aberto (só leitura)' },
  { arquivo: 'Main.dc.html', jaTem: 'st.consumoReal', de: 'consumo: [', para: 'consumo: st.consumoReal || [', motivo: 'consumo por funcionário (motor de tokens)' },
  { arquivo: 'Main.dc.html', jaTem: 'st.skillsReais', de: 'skillsAgora: [', para: 'skillsAgora: st.skillsReais || [', motivo: 'skills usadas agora (motor de tokens)' },
]
const lista = []
for (const nome of readdirSync(origem)) {
  if (PULAR.has(nome)) continue
  cpSync(join(origem, nome), join(destino, nome), { recursive: true })
}
const remendos = []
for (const r of REMENDOS) {
  const alvo = join(destino, r.arquivo)
  let t = readFileSync(alvo, 'utf8')
  if (t.includes(r.jaTem)) { remendos.push({ ...r, resultado: 'a fonte já faz' }); continue }
  const vezes = t.split(r.de).length - 1
  if (vezes !== 1) {
    remendos.push({ ...r, resultado: `NÃO APLICADO (achei ${vezes} vezes)` })
    console.error(`ATENÇÃO: remendo "${r.motivo}" não aplicou em ${r.arquivo} (o trecho aparece ${vezes} vezes). A tela vai mostrar o protótipo nessa parte.`)
    continue
  }
  writeFileSync(alvo, t.replace(r.de, r.para))
  remendos.push({ ...r, resultado: 'aplicado' })
}
const andar = (p) => {
  for (const n of readdirSync(p)) {
    const c = join(p, n)
    if (statSync(c).isDirectory()) andar(c)
    else lista.push({ arquivo: relative(destino, c).replaceAll('\\', '/'), bytes: statSync(c).size, sha1: createHash('sha1').update(readFileSync(c)).digest('hex').slice(0, 12) })
  }
}
andar(destino)
writeFileSync(join(raiz, 'ui', 'estudio.json'), JSON.stringify({ origem, sincronizadoEm: new Date().toISOString(), remendos: remendos.map((r) => ({ arquivo: r.arquivo, motivo: r.motivo, resultado: r.resultado })), arquivos: lista }, null, 2))
remendos.forEach((r) => console.log(`remendo: ${r.motivo} → ${r.resultado}`))
const total = lista.reduce((s, x) => s + x.bytes, 0)
console.log(`ui/estudio: ${lista.length} arquivos, ${(total / 1024 / 1024).toFixed(1)} MB, de ${origem}`)
if (!existsSync(join(destino, 'carregando.html'))) console.log('Aviso: sem carregando.html; o programa usa o carregamento provisório.')
