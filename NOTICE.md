# Avisos de terceiros

## painel-tokens-claude-vscode (Roger Fleming) — MIT

A regra de contagem de tokens e custo de `src-tauri/src/tokens.rs` foi portada de
`painel-tokens-claude-vscode/src/motor.js`, e `src-tauri/precos.json` é a tabela de preços desse projeto.

    MIT License

    Copyright (c) 2026 Roger Fleming

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.

As ideias de sessão viva (`~/.claude/sessions`), limite do plano (`~/.claude.json`), relógio do cache e
mapa de agentes vêm da leitura do `oficina` (MIT, © OFICINA contributors); nenhum código dele foi copiado.

## Claude Agent SDK (Anthropic)

`sidecar/` usa `@anthropic-ai/claude-agent-sdk` 0.3.318, com os termos da Anthropic
("SEE LICENSE IN README.md" do pacote). Ele não é redistribuído no repositório (`node_modules` fica fora);
o instalador leva uma cópia para o sidecar funcionar. Conferir os termos antes de distribuir para outras pessoas.

## Tauri (MIT/Apache-2.0) e crates oficiais

tauri, tauri-plugin-notification, tauri-plugin-dialog, tauri-plugin-single-instance, serde, serde_json,
getrandom, windows-sys — licenças MIT/Apache-2.0 dos próprios projetos.
