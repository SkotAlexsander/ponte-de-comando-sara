// Gera src-tauri/icons/fonte.png (1024×1024): a mira da placa da Main — dois anéis ciano,
// quatro marcas e o centro âmbar — num quadrado escuro arredondado. Sem biblioteca: PNG feito à mão.
// Depois: `npx tauri icon src-tauri/icons/fonte.png` gera o .ico e os PNGs (o `npm run icone` faz os dois).
import { writeFileSync, mkdirSync } from 'node:fs'
import { deflateSync } from 'node:zlib'
import { join, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const N = 1024
const S = N / 24 // a mira foi desenhada num viewBox de 24
const CIANO = [111, 243, 255], AMBAR = [255, 165, 58], FUNDO = [11, 16, 20], BORDA = [32, 52, 60]

// distâncias (em unidades do viewBox) → cobertura com borda suave
const anel = (d, r, l) => Math.abs(d - r) - l / 2
const cobre = (dist, px) => Math.max(0, Math.min(1, 0.5 - dist / px))

const img = Buffer.alloc(N * N * 4)
const px = 24 / N // 1 pixel em unidades do viewBox
for (let y = 0; y < N; y++) {
  for (let x = 0; x < N; x++) {
    const u = (x + 0.5) / S, v = (y + 0.5) / S
    // quadrado arredondado de fundo (margem 0,6, raio 5)
    const m = 0.6, r = 5
    const qx = Math.max(Math.abs(u - 12) - (12 - m - r), 0), qy = Math.max(Math.abs(v - 12) - (12 - m - r), 0)
    const dQuad = Math.hypot(qx, qy) - r
    const aFundo = cobre(dQuad, px)
    const aBorda = cobre(Math.abs(dQuad + 0.25) - 0.25, px) * aFundo
    const d = Math.hypot(u - 12, v - 12)
    const l = 1.15 // traço mais grosso que o da placa, para ler em 16 px
    let aCiano = Math.max(cobre(anel(d, 8.2, l), px), cobre(anel(d, 4.4, l), px))
    // marcas: de 1,6 a 4,4 de distância da borda, nos 4 lados
    const marca = (a, b) => { const along = Math.abs(a - 12), across = Math.abs(b - 12); return Math.max(across - l / 2, Math.max(along - 10.6, 7.6 - along)) }
    aCiano = Math.max(aCiano, cobre(marca(u, v), px), cobre(marca(v, u), px))
    const aAmbar = cobre(d - 2.0, px)
    const brilho = Math.exp(-((d - 2) ** 2) / 4) * 0.35 // halo âmbar
    let cor = FUNDO.map((c, i) => c * (1 - aBorda) + BORDA[i] * aBorda)
    cor = cor.map((c, i) => c + (AMBAR[i] - c) * Math.min(1, brilho) * (d > 2 ? 1 : 0))
    cor = cor.map((c, i) => c * (1 - aCiano) + CIANO[i] * aCiano)
    cor = cor.map((c, i) => c * (1 - aAmbar) + AMBAR[i] * aAmbar)
    const o = (y * N + x) * 4
    img[o] = cor[0]; img[o + 1] = cor[1]; img[o + 2] = cor[2]; img[o + 3] = Math.round(255 * aFundo)
  }
}

// PNG: assinatura + IHDR + IDAT + IEND
const crcTab = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0 })
const crc = (b) => { let c = 0xffffffff; for (const x of b) c = crcTab[(c ^ x) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0 }
const bloco = (tipo, dados) => { const t = Buffer.from(tipo); const len = Buffer.alloc(4); len.writeUInt32BE(dados.length); const c = Buffer.alloc(4); c.writeUInt32BE(crc(Buffer.concat([t, dados]))); return Buffer.concat([len, t, dados, c]) }
const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(N, 0); ihdr.writeUInt32BE(N, 4); ihdr[8] = 8; ihdr[9] = 6
const linhas = Buffer.alloc(N * (N * 4 + 1))
for (let y = 0; y < N; y++) { linhas[y * (N * 4 + 1)] = 0; img.copy(linhas, y * (N * 4 + 1) + 1, y * N * 4, (y + 1) * N * 4) }
const png = Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), bloco('IHDR', ihdr), bloco('IDAT', deflateSync(linhas, { level: 9 })), bloco('IEND', Buffer.alloc(0))])
const destino = join(raiz, 'src-tauri', 'icons', 'fonte.png')
mkdirSync(dirname(destino), { recursive: true })
writeFileSync(destino, png)
console.log(`ícone: ${destino} (${(png.length / 1024).toFixed(0)} KB)`)
