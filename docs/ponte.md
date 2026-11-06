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
| `conversa(conversa)` | `{ sessao, iniciada, modelo, esforco, rapido, pasta, criadaEm, atualizadaEm, rodando }` ou `null` (`rodando`: o turno ainda está em andamento; a Main usa ao recarregar) |
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
| `resultado` | `texto`, `erro`, `custo` (estimado pelo SDK), `duracaoMs`, `tokensEntrada`, `tokensSaida`; `cancelado: true` (com `erro: false`) quando é o fim do turno que o Alex PAROU: não é erro nem pronto |
| `erro` | `texto` |
| `cancelado` / `fechada` | |

**Permissão:** `aoPermissao({ conversa, pedido, ferramenta, frase, detalhe })`, por exemplo com `frase: 'Escrever o arquivo X'` ou `'Rodar um comando: …'`, e depois `aoPermissaoFim({ pedido })`. Responda com `responderPermissao(pedido, true|false)`. Enquanto o pedido espera, a miniatura avisa "Alex, preciso de você."

## Configurações

```js
const c = await ponte.config.ler()
// → { tema: 'metal'|'fosforo'|'grade', fosforo: 'ambar'|'verde'|'branco', fosforoLetra: 'terminal'|'atual',
//     pastaTrabalho: 'A:\\Claude', pastaTrabalhoPadrao: true, pastaTrabalhoCaminhoPadrao: 'A:\\Claude', aprovacaoSempre: true,
//     miniAoMinimizar: true, toastWindows: 'auto'|'sempre'|'nunca', mudo: false, …as chaves novas abaixo }
await ponte.config.gravar({ tema: 'fosforo', fosforo: 'verde' })   // → a config inteira depois de gravar
ponte.aoConfig((c) => { … })                                         // todas as janelas recebem quando alguma grava
```

- O arquivo é `%APPDATA%\com.pixelmartins.pontedecomando\config.json`.
- `gravar` aceita **só** as chaves acima, e chave ou valor desconhecido rejeita com mensagem em PT-BR, sem gravar nada.
- `pastaTrabalho: null` volta ao padrão; texto precisa ser uma pasta que existe.
- `miniAoMinimizar` é o mesmo item da bandeja.
- `toastWindows`:
  - `auto`: só quando nem a principal nem a miniatura estão na tela;
  - `sempre`: o item "Notificação do Windows sempre" da bandeja;
  - `nunca`.
  - **Fora do programa instalado nunca sai toast**, porque ele apareceria como "Windows PowerShell".
- **Tema:** a miniatura e a Main recebem `data-tema` e `data-fosforo` no `<html>`. Na Main, a Sara aplica o atributo na área da conversa. O tema muda **só** o que fica dentro do vidro da moldura da conversa.

- **Só leitura:** `pastaTrabalhoPadrao`, `pastaTrabalhoCaminhoPadrao` e `aprovacaoSempre`. Mandar de volta o objeto do `ler()` não dá erro por causa delas (são ignoradas). `aprovacaoSempre` só aceita `true`: **nenhuma chave desliga o cartão de permissão**, e chaves como `permissionMode`, `pularAprovacao` etc. são recusadas como desconhecidas.
- `pastaTrabalho` só aceita `null` (voltar ao padrão). Trocar a pasta é **só** pelo seletor do Windows (`trocarPasta()`), para a página não mudar a barreira dos arquivos (QA M7).
- `fosforo` padrão `branco` e `fosforoLetra` padrão `atual`: a combinação aprovada pelo Alex. A Main recebe `data-tema`, `data-fosforo`, `data-fosforo-letra`, `data-idioma`, `data-letra` e `data-animacoes` no `<html>`.

### Chaves novas (opções inteligentes)

| Chave | Valores | O que faz |
|---|---|---|
| `idioma` | `pt-BR` \| `en` | língua da interface; as mensagens de erro novas saem nela |
| `idiomaRespostas` | `seguir` (a da interface) \| `pt-BR` \| `en` | língua das respostas do Claude (ver abaixo) |
| `tamanhoLetra` | 90 \| 100 \| 110 \| 125 | guardado para a tela aplicar |
| `animacoes` | `sistema` \| `sempre` \| `reduzidas` | guardado para a tela aplicar |
| `padraoNova` | `{ modelo: opus\|sonnet\|haiku, esforco: low…max (aceita BAIXO…MÁXIMO), rapido }` | usado quando a aba não manda modelo/esforço/velocidade |
| `lembrarAprovacoes` | bool | ao PERMITIR no cartão, devolve a regra que o próprio Claude Code sugeriu (vale até o fim da sessão) |
| `somAviso`, `volumeAviso` | bool, 0–100 | som da miniatura e da Main |
| `cantoMini` | `ie` \| `id` \| `se` \| `sd` | canto da miniatura |
| `abrirComWindows` | bool | plugin oficial autostart (abre com `--minimizado`) |
| `comecarNaBandeja` | bool | abre sem carregamento e sem principal: só a miniatura |
| `modoConta` | `assinatura` \| `api` | `api` exige uma chave guardada |

**Idioma das respostas:** com `pt-BR` ou `en` efetivo, a conversa leva três coisas:
- o **append** no fim do system prompt do Claude Code (`systemPrompt: { type: 'preset', preset: 'claude_code', append, snapshot: false }`; o prompt do Claude Code nunca é trocado);
- a configuração oficial `language` do Claude Code;
- uma **marca curta no começo da mensagem** enviada ao Claude, como `[Ponte de Comando: answer in English]`. A tela não mostra a marca.

O motivo foi medido em 06/11/2026, com o Haiku e com o Sonnet: o append e o `language` sozinhos perdem para a língua em que o Alex escreve; com a marca, o Claude obedece. Trocar o idioma no meio de uma conversa reabre a sessão retomando (vale já na próxima mensagem).

## Conta (`ponte.conta`)

| Função | |
|---|---|
| `status()` | `{ ok, logado, metodo, email: 's***@dominio', organizacao, plano, modo, temChave }` (`claude auth status --json`) |
| `modo()` | `'assinatura'` \| `'api'` |
| `trocar()` | abre o **login oficial** (`claude auth login`) num terminal **visível**; quando ele fecha, `aoConta(fn)` recebe o status novo |
| `sair(true)` | logout oficial (`claude auth logout`); sem `true` é recusado (a tela confirma antes) |
| `guardarChave(chave)` | grava no **Cofre do Windows** (Gerenciador de Credenciais, alvo `PonteDeComando/anthropic-api-key`); recusa o que não tiver o formato `sk-ant-…` |
| `testarChave()` | chamada mínima e gratuita (`GET /v1/models`) → `{ ok, status, erro?, modelos? }` |
| `apagarChave()` | tira do Cofre; se o modo era `api`, volta para `assinatura` |

Como a chave é tratada:
- Ela **nunca** vai para arquivo, registro, linha de comando, evento ou tela.
- No modo `api`, ela entra só na variável `ANTHROPIC_API_KEY` do processo do sidecar e do teste.
- No modo `assinatura`, essa variável é **tirada** do sidecar, mesmo que exista no Windows.
- Trocar de modo reabre o sidecar quando nenhuma conversa está rodando.
- O Cofre é usado direto pela API do Windows (`CredWriteW`/`CredReadW`/`CredDeleteW`), sem biblioteca de terceiro.
- O modo `claude -p` (sem Node) usa só o login do Claude Code.

## Sistema (`ponte.sistema`)

| Função | |
|---|---|
| `abrirPastaDados()` | abre no Explorer a pasta de dados do programa (caminho fixo) |
| `exportarConfig()` | pergunta onde salvar → `{ programa: 'Ponte de Comando', versaoConfig: 1, config }` (sem token, porta nem chave) |
| `importarConfig()` | escolhe o arquivo, valida **tudo** e só então aplica; arquivo de outro programa, JSON inválido ou valor errado é recusado inteiro |
| `restaurarPadrao(true)` | volta tudo que a tela configura ao padrão e desliga o início com o Windows |

## Miniatura e janela principal

- A miniatura **some** quando a principal está na tela e **aparece** quando ela é minimizada, vai para a bandeja (fechar) ou ainda não abriu, se "Miniatura ao minimizar" estiver ligada. Há um debounce de 150 ms, para não piscar, e a regra é reconferida a cada 3 s.
- Clicar na miniatura traz a principal (`mostrarPrincipal()`), e a miniatura some.
- `estado().onde` diz onde está o aviso agora:
  - `principal`: a Main toca o som e mostra na placa;
  - `mini`;
  - `nenhum`: vira toast se `toastWindows` deixar.

## Estado da Sara e avisos

| Função | |
|---|---|
| `estado()` | `{ estado: 'esperando'\|'trabalhando'\|'precisa'\|'pronto'\|'erro', legenda, pedido, projeto, ferramenta, aviso: { tipo, topo, titulo, texto, acao, em, sessao } \| null, som, mudo, onde, alertas: [texto], em }`. `alertas`: avisos do programa (porta dos hooks ocupada, pasta de trabalho gravada que sumiu), que a Main mostra numa faixa. Uma sessão trabalhando não apaga o aviso de **outra** sessão |
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

## Segurança das janelas (QA M8/B6)

- **CSP** em `tauri.conf.json`: só scripts do próprio programa (`'unsafe-eval'` porque o runtime do protótipo usa `new Function`), estilos próprios + Google Fonts, fontes do `fonts.gstatic.com`, imagens e vídeos locais (`data:`/`blob:`), nada de `object`, `base` ou `form`. Script injetado de fora não roda.
- **Cada janela só chama o que usa** (`src-tauri/capabilities/`): a `principal` tem todos os comandos; a `mini` só `estado_atual`, `config_ler`, `definir_mudo`, `obter_mudo`, `dispensar_aviso`, `mostrar_principal`, `mini_tamanho`, `mini_arrastando` e `mini_canto`; o `carregamento` só `carregamento_concluido` e `info_app`. **Comando novo** precisa entrar em `src-tauri/build.rs` (lista `COMANDOS`) e numa capability, senão a janela recebe "not allowed".
- Janela escondida ou minimizada: a casca avisa o WebView2 (`SetIsVisible`) e a página (`document.hidden` passa a `true`); vídeos e animações param e a raiz fica com `visibility: hidden`. A Main não relê o consumo escondida.
