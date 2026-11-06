# Ponte de Comando · Sara

Programa do Alex para acompanhar o Claude Code no PC. Ao abrir, aparece a tela de carregamento da Sara por pelo menos 8 segundos. Depois abrem a janela principal e a **miniatura da Sara**, que fica no canto da tela, por cima de tudo, e avisa quando o trabalho fica pronto ou quando ela precisa de você.

## O que ele faz hoje

- **Carregamento** sem borda, 960×560, centralizado, com a arte `carregando.html` da Sara (≥ 8 s).
- **Janela principal**: a conversa (`Main`) e, pelo botão FLUXO, o mapa (`Fluxo`); "← CONVERSA" volta. Fechar só esconde, e o programa continua na bandeja.
- **Conversa de verdade** com o Claude Code, uma sessão por aba, cada aba com o seu **MODELO**, **ESFORÇO** e **VELOCIDADE** (modo rápido). A resposta aparece enquanto é escrita, e as ferramentas usadas viram a lista de atividades. PARAR interrompe sem perder a conversa.
- **Cartão de permissão** em português ("Escrever o arquivo X", "Rodar um comando: …") antes de qualquer ação. O programa **nunca** age sem pedir.
- **Pasta de trabalho**: a padrão é `A:\Claude`, e dá para trocar pelo botão PASTA e voltar ao padrão. A árvore de ARQUIVOS é a real, aberta uma pasta por vez (sem `node_modules`, `.git` e `target`). O **+** cria pasta ou arquivo em Arquivos e na Biblioteca (Cérebro, Alexandria) e nunca sobrescreve. Apagar não existe.
- **Miniatura**: a Sara em vídeo com a linha de estado (esperando, trabalhando, precisa de você, pronto, erro). No pronto, mostra o aviso "Alex, seu trabalho está pronto." com VER AGORA e DEPOIS, toca um som curto e manda uma notificação do Windows. Clicar nela traz a principal. Para mudar de canto, é só arrastar: ela gruda no canto mais perto e lembra onde ficou. Não aparece na barra de tarefas e não rouba o foco.
- **Bandeja**: Abrir · Mostrar/Esconder a miniatura · Silenciar avisos · Sair.
- **Avisos de conversas de fora** (terminal, VS Code): pelos hooks (`docs\hooks.md`, ainda **não instalados**) e pela mudança `busy` → `idle` em `~/.claude/sessions`.
- **Consumo**: tokens e custo **estimado** de cada conversa viva, por subagente, por modelo e por skill (motor portado do painel do Rogério); limites de 5 h e 7 dias de `~/.claude.json`; relógio do cache; conta e conexões MCP pela `window.ponte`.

## Pré-requisitos

Já estão nesta máquina: Rust MSVC (`C:\Users\alexs\.rustup`), VS Build Tools 2022, SDK do Windows 10.0.26100, WebView2, **Node 24** e o **Claude Code** em `C:\Users\alexs\.local\bin\claude.exe`, com login feito. O Node é **obrigatório** para conversar pelo programa. Sem ele, a conversa cai para o `claude -p`, sem cartão de permissão.

## Rodar

```
npm install                 # uma vez: o CLI do Tauri
npm --prefix sidecar install --omit=optional   # uma vez: o Claude Agent SDK do sidecar
npm run dev                 # compila e abre o programa (carregamento → principal → miniatura)
npm run build               # gera o instalador NSIS
npm run sincronizar-ui      # recopia a interface da Sara (rode sempre que ela mudar o protótipo)
npm run icone               # regenera o ícone (mira ciano, centro âmbar)
```

O instalador sai em `src-tauri\target\release\bundle\nsis\Ponte de Comando_0.1.0_x64-setup.exe`. Para atualizar, **saia pela bandeja antes** (Sair), senão o instalador não consegue trocar o executável.

**O Smart App Control está ligado** e às vezes barra executável sem assinatura. Se barrar, não contorne: o arquivo fica onde está e a solução de verdade é assinar o código (ver `docs\P00.md`).

## Testes

```
npm run testar              # hook (node:test) + cargo test (arquivos, servidor, estado, conversa, tokens)
npm run testar:programa     # o .exe de verdade, isolado: carregamento, janelas, árvore, criar, hook → miniatura,
                            # conversa real com Haiku, cartão de permissão, tokens, limites (gasta centavos)
node ferramentas\testes\programa.mjs --sem-claude   # o mesmo, sem a conversa real
npm run testar:sidecar-real # só o sidecar: sessão nova, segunda mensagem, retomar depois de fechar
```

Os testes do programa usam pasta de dados, perfil do WebView2, pasta de trabalho e porta **próprios**, em `%TEMP%\ponte-teste-programa`. Nunca tocam nos dados de verdade. As capturas ficam em `docs\capturas\`.

## Instalar o hook do Claude Code

Leia `docs\hooks.md`: o bloco exato para colar no `C:\Users\alexs\.claude\settings.json` e o teste:

```
node ferramentas\hook\ponte-hook.mjs --verboso < ferramentas\hook\exemplo-stop.json
```

**Não foi instalado.** Mudar o `settings.json` é decisão do Alex.

## Onde fica cada coisa

| Caminho | O que é |
|---|---|
| `ui\estudio\` | **cópia** do protótipo da Sara (`00-empresa\corpos\sara\interface\estudio-local\`); não edite aqui, rode `npm run sincronizar-ui` |
| `ui\estudio.json` | o que foi copiado, quando, e quais remendos de uma linha entraram |
| `ui\ponte.js` | `window.ponte`, o contrato da interface com o programa (`docs\ponte.md`) |
| `ui\ponte-main.js` | adaptador provisório da Main (conversa, árvore, consumo, placa, cartão) |
| `ui\mini.html` | a miniatura (visual copiado do `Mini.dc.html`) |
| `ui\carregamento.html` | abre `estudio\carregando.html` e espera o "carregado" (≥ 8 s) |
| `src-tauri\src\lib.rs` | janelas, bandeja, comandos, ligação de tudo |
| `src-tauri\src\arquivos.rs` | pasta de trabalho: listar, criar e ler, com as barreiras |
| `src-tauri\src\servidor.rs` | servidor local dos hooks |
| `src-tauri\src\estado.rs` | o estado da Sara e os avisos |
| `src-tauri\src\conversa.rs` · `sidecar.rs` | conversas: modelo/esforço/velocidade por aba, sidecar, modo `claude -p` |
| `src-tauri\src\tokens.rs` · `precos.json` | motor de tokens, sessões vivas, limites, conta, MCP |
| `sidecar\ponte-sidecar.mjs` | o processo Node com o Claude Agent SDK |
| `ferramentas\hook\` | o hook (`ponte-hook.mjs`), a tradução (`mapear.mjs`) e os exemplos |
| `ferramentas\testes\` | os testes |
| `docs\P00.md` | fases, decisões e os **[decidir]** |
| `docs\ponte.md` | o contrato `window.ponte` |
| `docs\hooks.md` | o bloco dos hooks e o teste |
| `docs\pesquisa\` | relatórios do Heitor (não é do código) |
| `NOTICE.md` | avisos de licença (Rogério, SDK, Tauri) |

**Dados do programa:** `%APPDATA%\com.pixelmartins.pontedecomando\`, com:
- `config.json`: porta, pasta de trabalho, mudo, canto e monitor da miniatura;
- `token` e `porta`: usados pelo hook;
- `conversas.json`: aba → sessão, modelo, esforço, velocidade e pasta;
- `ponte.log`: registro, sem conteúdo de conversa.

Nos testes, `PONTE_PASTA_DE_DADOS` troca essa pasta inteira.
