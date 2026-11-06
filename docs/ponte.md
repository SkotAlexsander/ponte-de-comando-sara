# `window.ponte`: o contrato entre a interface e o programa

A casca injeta `ui/ponte.js` em **todas** as janelas antes de qualquer script da página. Por isso
`window.ponte` já existe quando o `renderVals` da Main roda. Fora do programa (no navegador ou nos testes),
`window.ponte` também existe, mas é **de mentira**: os dados ficam em memória e `ponte.noPrograma` vale `false`.

Regras:
- Toda função devolve uma `Promise`. Quando dá erro, a promessa é **rejeitada com um `Error` cuja mensagem está em PT-BR**, pronta para mostrar embaixo do campo.
- Todo `aoAlgo(fn)` devolve a função que desliga o ouvinte.
- Tudo que vem do Claude (texto, nome de arquivo, descrição de agente) é **dado**. Mostre com `textContent` e nunca com `innerHTML`.

## Pasta de trabalho e arquivos

| Função | Devolve | Observação |
|---|---|---|
| `pastaDeTrabalho()` | `{ caminho, nome, padrao, caminhoPadrao }` | padrão `A:\Claude` |
| `trocarPasta()` | `string` (caminho escolhido) ou `null` se cancelar | abre o seletor do Windows e grava a escolha |
| `voltarPadrao()` | `string` (o caminho padrão) | |
| `listar(pasta)` | `{ pasta, absoluto, itens: [{ nome, caminho, absoluto, tipo: 'pasta'\|'arquivo', link, tamanho }], truncado }` | uma pasta por vez; ignora `node_modules`, `.git` e `target`; até 2000 itens |
| `criarPasta(destino, nome)` | o item criado | nunca sobrescreve |
| `criarArquivo(destino, nome)` | o item criado (vazio) | nunca sobrescreve |
| `_invocar('ler_arquivo', { caminho })` | texto | só leitura, até 256 KB, recusa binário |

**O `destino` (e o `pasta` do `listar`) aceita, nesta ordem de preferência:**
1. **caminho absoluto** dentro da pasta de trabalho (`A:\Claude\03-projetos\x\`). É o mais limpo, porque não tem ambiguidade;
2. caminho **relativo à pasta de trabalho** (`03-projetos\x\`, e `""` é a própria pasta). Também vale com o nome da raiz na frente (`Claude\03-projetos\x\`), que é o que a árvore da Main monta hoje;
3. `Biblioteca\<Estante>\...`:
   - `Cérebro` → `A:\Claude\02-cerebro`;
   - `Alexandria` → `A:\Claude\03-projetos\alexandria`;
   - `Memória` é **recusada**, porque mora em `C:\Users\alexs\.claude\projects\a--Claude\memory`, fora da pasta de trabalho.

   Se a pasta de trabalho não for a padrão, as estantes ficam de fora e a criação é recusada com uma mensagem dizendo isso.

**Barreiras no Rust:** tudo passa por `canonicalize` e precisa ficar dentro da pasta de trabalho. Isso recusa `..`, caminho de outro disco, `C:algo`, caminho de rede e atalho/junção que aponte para fora. O nome é validado de novo (vazio, `\ / : * ? " < > |`, caractere de controle, `CON`/`PRN`/`AUX`/`NUL`/`COM1-9`/`LPT1-9` com ou sem extensão, ponto no fim, mais de 200 letras). Nome repetido, com qualquer caixa, é recusado. **Apagar não existe.**

## Conversa (uma sessão do Claude Code por aba)

```js
await ponte.enviar({
  conversa: 'landing@k3f9',      // id da aba: letras, números, - _ . @ :
  texto: 'Faz a landing…',
  modelo: 'OPUS',                // OPUS | SONNET | HAIKU (ou opus/sonnet/haiku, ou um id claude-…)
  esforco: 'ALTO',               // BAIXO | MÉDIO | ALTO | MÁXIMO (ou low/medium/high/xhigh/max)
  velocidade: 'NORMAL',          // NORMAL | RÁPIDA   (ou rapido: true/false)
  pasta: ''                      // onde a sessão roda (só na 1ª mensagem); '' = pasta de trabalho
})  // → { conversa, sessao, nova, modelo, esforco, rapido, pasta, motor: 'sdk' | 'cli' }
```

**Como isso chega ao backend:**
- O Rust guarda, por `conversa`, a sessão (um UUID), o modelo, o esforço, a velocidade e a pasta, em `conversas.json` na pasta de dados.
- O Rust repassa ao **sidecar** (`sidecar/ponte-sidecar.mjs`, Node + `@anthropic-ai/claude-agent-sdk` 0.3.318, usando o `claude.exe` instalado). O sidecar faz `query()` com:
  - `model`, `effort` e `settings: { fastMode }`;
  - `sessionId` (na 1ª mensagem) ou `resume` (depois de reabrir o programa);
  - `permissionMode: 'default'`, que é **sempre** este e nunca o que age sem pedir;
  - `settingSources: ['user','project','local']`, para valerem o CLAUDE.md, o `deny` e os hooks da pasta;
  - `includePartialMessages`, para a resposta aparecer ao vivo.
- Enquanto a sessão está viva, a próxima mensagem da mesma aba entra na mesma `query`. Se a aba trocou o modelo, o sidecar chama `setModel()`. Se trocou o esforço ou a velocidade, chama `applyFlagSettings({ effortLevel, fastMode })`.
- Sem Node ou sem o sidecar, o Rust cai para `claude -p --output-format stream-json --verbose --model … --effort … --settings {"fastMode":…} --permission-mode default (--session-id|--resume) <uuid>`, com o texto pelo stdin. Nesse modo **não há cartão de permissão**: o que não estiver liberado nos settings é negado.

| Função | Faz |
|---|---|
| `parar(conversa)` | interrompe o turno (`interrupt()`) **sem perder a conversa** |
| `conversa(conversa)` | `{ sessao, iniciada, modelo, esforco, rapido, pasta, criadaEm, atualizadaEm }` ou `null` |
| `responderPermissao(pedido, permitir, motivo?)` | responde ao cartão |
| `pararAgente(conversa, tarefa)` | para um subagente (`stopTask`) |
| `conversasAntigas(pasta?, limite?)` | `{ pasta, conversas: [{ sessao, nome, atualizadaEm, bytes, pasta }] }` |
| `retomarSessao(conversa, sessao, pasta)` | liga uma aba a uma sessão antiga; a próxima mensagem retoma por `resume` |

**Eventos `aoConversa(ev)`** (com `ev.conversa` sempre presente):

| `ev.tipo` | Campos |
|---|---|
| `inicio` | `sessao`, `modelo`, `permissao` |
| `delta` | `texto` (pedaço da resposta, ao vivo) |
| `texto` | `texto` (bloco completo, que substitui os deltas) |
| `ferramenta` | `nome`, `legenda` ("Lendo LEIA-ME.md"), `detalhe`, `pai` (id do agente-pai, se veio de subagente) |
| `agente` | `subtipo` (task_started/progress/updated/notification), `tarefa`, `descricao`, `estadoTarefa`, `uso` |
| `limite` | `info` (rate_limit_event do SDK; **fração**, não %) |
| `resultado` | `texto`, `erro`, `custo` (estimado pelo SDK), `duracaoMs`, `tokensEntrada`, `tokensSaida` |
| `erro` | `texto` |
| `cancelado` / `fechada` | |

**Permissão:** `aoPermissao({ conversa, pedido, ferramenta, frase, detalhe })`, por exemplo com `frase: 'Escrever o arquivo X'` ou `'Rodar um comando: …'`, e depois `aoPermissaoFim({ pedido })`. Responda com `responderPermissao(pedido, true|false)`. Enquanto o pedido espera, a miniatura avisa "Alex, preciso de você."

## Estado da Sara e avisos

| Função | |
|---|---|
| `estado()` | `{ estado: 'esperando'\|'trabalhando'\|'precisa'\|'pronto'\|'erro', legenda, pedido, projeto, ferramenta, aviso: { tipo, topo, titulo, texto, acao, em } \| null, som, mudo, em }` |
| `aoEstado(fn)` | muda com os hooks, as sessões vivas (`busy` → `idle`) e as conversas do programa |
| `dispensarAviso()` · `mudo()` · `definirMudo(b)` · `aoMudo(fn)` · `mostrarPrincipal()` | |

## Números (motor de tokens; custo sempre **estimado**)

| Função | Devolve |
|---|---|
| `tokens()` / `aoTokens(fn)` | `{ sessoes: [{ sessao, nome, pid, cwd, status, modelo, contexto, totalProcessado, materialNovo, entrada, saida, leituraCache, escritaCache, custo, custoEstimado: true, porModelo[], agentes[{ chamada, id, nome, tipo, pai, inicio, estado: 'rodando'\|'terminou', tokens, custo }], skills[{ nome, vezes }], modelosDesconhecidos[], cache: { desde, ttlMinutos, suposto } }], total, precosConferidos: false }` |
| `limites()` | `{ disponivel, cincoHoras: { porcento, zeraEm }, seteDias: {…}, lidoEm, fonte }`, lidos de `~/.claude.json`. Leitura que falha nunca apaga a anterior |
| `conta()` | `{ ok, logado, metodo, email, organizacao, plano }`, de `claude auth status --json` |
| `conexoes()` | `{ ok, conexoes: [{ nome, estado: 'conectado'\|'precisa de login'\|'falhou', detalhe }], bruto }`, de `claude mcp list` |
| `info()` | versão, porta, pasta de dados, motor da conversa, node, janelas visíveis |

## O que a Main ainda faz por adaptador (provisório)

`ui/ponte-main.js` entra **só** na janela principal e liga os dados reais na lógica do protótipo sem editar a fonte. Cada parte se desliga sozinha quando o template passa a fazer por conta própria:

| Parte | Como liga hoje | Desliga quando a fonte da Main… |
|---|---|---|
| Conversa | segura Enter/ENVIAR/PARAR e usa `ponte.enviar` com o MODELO/ESFORÇO/VELOCIDADE da aba; grava em `st.extra`, `st.estado` e `st.rodandoEm` | …tiver `ponte.enviar` |
| Árvore de Arquivos | remendo `const NOS = st.nosReais \|\| [...]` + `ponte.listar` | …tiver `st.nosReais` |
| Arquivo aberto | remendo `const CONTEUDO = st.conteudoReal \|\| {...}` | …tiver `st.conteudoReal` |
| Consumo e skills | remendos `consumo: st.consumoReal \|\| [...]` e `skillsAgora: st.skillsReais \|\| [...]`; o topo do painel (62 %, 184k…) é texto fixo e é trocado só no texto | …tiver `st.consumoReal` / `st.skillsReais` |
| Placa "SARA · …" | `aoEstado` → `st.estado` (precisa → `escutando`) | (sempre) |
| Cartão de permissão | caixa própria por cima da conversa, com foco começando em NEGAR | (até a Main ter o dela) |

Os remendos são aplicados pelo `ferramentas/sincronizar-ui.mjs` **na cópia** `ui/estudio/` e registrados em `ui/estudio.json`. Se um trecho mudar na fonte, o sincronizador avisa: "remendo … NÃO APLICADO".

**Pedido para a fonte da Main (Sara):** levar para o próprio template o `ponte.enviar`/`aoConversa`, a árvore por `ponte.listar` (com o `caminho` absoluto do item no `destino`), o consumo por `ponte.tokens()` e um cartão de permissão no estilo dela. O Fluxo ainda não tem dados: o mapa de agentes vivo está em `tokens().sessoes[].agentes` e nos eventos `agente`.
