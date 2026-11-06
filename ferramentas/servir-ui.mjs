// Servidor estático mínimo da pasta ui/ (só 127.0.0.1), para testar as páginas no navegador sem o programa.
// Uso: node ferramentas/servir-ui.mjs [porta]   → http://127.0.0.1:5199/estudio/Main.dc.html
import { createServer } from 'node:http'
import { createReadStream, existsSync, statSync } from 'node:fs'
import { join, normalize, resolve, extname, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'ui')
const TIPOS = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.json': 'application/json', '.css': 'text/css', '.webp': 'image/webp', '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml', '.mp4': 'video/mp4', '.ico': 'image/x-icon' }

export function servir(porta = 5199) {
  const srv = createServer((req, res) => {
    const caminho = decodeURIComponent(new URL(req.url, 'http://x').pathname)
    const alvo = normalize(join(raiz, caminho))
    if (!alvo.startsWith(raiz) || !existsSync(alvo) || statSync(alvo).isDirectory()) { res.writeHead(404); res.end('404'); return }
    const tam = statSync(alvo).size
    const tipo = TIPOS[extname(alvo).toLowerCase()] || 'application/octet-stream'
    const faixa = /bytes=(\d*)-(\d*)/.exec(req.headers.range || '')
    if (faixa) {
      const ini = faixa[1] ? +faixa[1] : 0, fim = faixa[2] ? +faixa[2] : tam - 1
      res.writeHead(206, { 'Content-Type': tipo, 'Content-Range': `bytes ${ini}-${fim}/${tam}`, 'Accept-Ranges': 'bytes', 'Content-Length': fim - ini + 1 })
      createReadStream(alvo, { start: ini, end: fim }).pipe(res)
    } else {
      res.writeHead(200, { 'Content-Type': tipo, 'Content-Length': tam, 'Accept-Ranges': 'bytes' })
      createReadStream(alvo).pipe(res)
    }
  })
  return new Promise((ok) => srv.listen(porta, '127.0.0.1', () => ok(srv)))
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const porta = +(process.argv[2] || 5199)
  await servir(porta)
  console.log(`ui em http://127.0.0.1:${porta}/estudio/Main.dc.html`)
}
