# O que os programas do Rogério fazem, e o que a Ponte precisa fazer igual

**Quem fez:** Heitor (segurança) · **Data:** 05/11/2026 · **Tipo:** leitura em quarentena, nada instalado nem executado
**Para quê:** o Alex quer que a Ponte de Comando · Sara faça "todas as coisas que a versão do Mano já faz, e mais as do meu estilo". Este documento é o checklist dessas coisas, com o lugar exato de cada uma no código dele.

> Os números de linha valem para os commits auditados (tabela abaixo). Se o Rogério mudar o repo, as linhas andam.

---

## 1. O resultado primeiro

| Repo | Licença | Commit auditado | Veredito | Por quê, em uma linha |
|---|---|---|---|---|
| `painel-tokens-claude-vscode` | MIT © 2026 Roger Fleming | `314fbf28` | **Pode servir de base** (a lógica) | Só lê arquivos locais, nenhuma rede, tela sem injeção de HTML. É o melhor motor de tokens que existe por aqui. |
| `oficina` (extensão `oficina-claude`) | MIT © OFICINA contributors + Microsoft | `6eefd17f` | **Só referência**, com trechos copiáveis | É um editor VS Code inteiro. A Ponte não vai ser um VS Code, mas as ideias e as medições são ouro. Vem de fábrica sem pedir aprovação ao agente (ver §5). |
| `blender-3d-web` | **sem licença** | `054c1b32` | **Só referência** (não republicar) | Guia de texto para o boneco 3D da Sara. Nenhum código. |
| `claude-pombo` (privado) | sem licença | `a45181bc` | **Só referência**, e com cuidado | Canal em que um Claude lê o que o outro escreveu. O Claude do Alex já entrou nele (ver §5). |
| `pacote-paulo-claude` | sem licença | `e7686bd1` | **Evitar** para a Ponte | Fora do assunto (skills, DaVinci, CAD). O scanner reprovou por coisas que, lidas, são benignas. |
| `daedalus-status` | sem licença | `8140538d` | **Fora do escopo** | É o vigia da licença do Daedalus (proprietário). Nada para a Ponte. |

Não estudei: `daedalus`, `bene`, `roxin`, `kit-escritor`, `cenoura`, `ia-local`, `transcricoes-apometria` e `rancho-previa-mano`, porque nenhum deles tem a ver com Claude Code, painel ou agentes.

**O caminho que recomendo:** a Ponte faz o **motor do painel de tokens** reescrito em Rust (com o aviso MIT do Roger), a **detecção de sessão viva** de `~/.claude/sessions/` dos dois repos, o **limite de 5 h e de 7 dias** lido de `~/.claude.json` como o Oficina lê, e a **janelinha sempre por cima com bandeja**, como o painel faz. Para conversar com o Claude, há duas rotas (ver §4.2). As duas precisam de decisão do Alex.

---

## 2. Como os dois programas leem o Claude Code (o mapa dos dados)

Os dois leem **só arquivos que o próprio Claude Code grava no PC**. Nenhum deles chama a API da Anthropic nem pede chave.

| Arquivo | O que tem | Quem lê | Para quê |
|---|---|---|---|
| `~/.claude/projects/<pasta>/<sessão>.jsonl` | Uma linha JSON por evento da conversa | os dois | tokens, custo, modelo, nome, skills, agentes lançados |
| `~/.claude/projects/<pasta>/<sessão>/subagents/agent-<id>.jsonl` | O transcript de cada subagente | os dois | gasto de cada subagente (não aparece no arquivo principal) |
| `…/subagents/agent-<id>.meta.json` | `description`, `agentType`, `toolUseId` do subagente | Oficina | nome e tipo do agente em qualquer nível, e quem é o pai |
| `~/.claude/sessions/<pid>.json` | `pid`, `sessionId`, `cwd`, `entrypoint`, `kind`, `name`, `nameSource`, **`status` (`idle`/`busy`)**, `procStart`, `startedAt`, `updatedAt` | os dois | qual conversa está aberta **de verdade**, e se está trabalhando |
| `~/.claude.json` → `cachedUsageUtilization` | `five_hour` e `seven_day` com `utilization` (%) e `resets_at` | Oficina | limite do plano sem rede e sem custo |
| `claude auth status --json` | `loggedIn`, `authMethod`, `email`, `orgName`, `subscriptionType` | Oficina | tela da conta |
| `claude mcp list` / `claude mcp get <nome>` | Texto com o estado de cada servidor MCP | Oficina | tela das conexões |
| SDK `@anthropic-ai/claude-agent-sdk` 0.3.261 | `query()`, `accountInfo()`, `listSessions()`, `usage_EXPERIMENTAL…` | Oficina | conversa própria, contas, sessões antigas, limite ao vivo |

### 2.1 Como o painel conta tokens e custo (a regra que a Ponte deve copiar)

Tudo em `painel-tokens-claude-vscode/src/motor.js`:

1. **Lê só o pedaço novo do arquivo**, em blocos de 4 MB, guardando a última linha incompleta para a próxima volta (`motor.js:194-264`). Se o arquivo for trocado por outro, percebe pelo inode + data de criação e recomeça (`motor.js:221-225`).
2. **Uma resposta aparece em várias linhas** (uma por bloco de conteúdo), com o mesmo `message.id`. Nos subagentes a saída vem **parcial e crescendo**. Por isso ele fica com a linha de **maior `output_tokens`** de cada `message.id` e desfaz a soma anterior (`motor.js:482-506`). Contar a primeira linha joga fora a maior parte da saída, que é o token mais caro.
3. **Ignora** as respostas de modelo `<synthetic>` (`motor.js:333`).
4. **Custo de uma resposta** (`motor.js:60-83`), em US$ por milhão de tokens:
   `input × entrada + cache_read × entrada × 0,1 + escrita_5m × entrada × 1,25 + escrita_1h × entrada × 2 + output × saída + buscas_web × US$ 0,01`.
   Se `cache_creation` não traz a divisão 5m/1h, conta tudo como 5m (`motor.js:63-67`). Fable 5.1 e Mythos 5.1 leem cache a 0,025× (`motor.js:69-71`). O modo rápido (`usage.speed === "fast"`) usa a tabela `rapido` (`motor.js:36-51`).
5. **O modelo casa por prefixo, do mais longo para o mais curto**, porque o transcript grava `claude-haiku-4-5-20251001` (`motor.js:19-21` e `36-51`). Modelo desconhecido cai em US$ 5/25 e fica marcado (`motor.js:504`).
6. **Três números diferentes, e os três importam** (`sessoes.js:134-147`):
   - **contexto agora** = `input + cache_read + cache_creation` da **última** resposta do transcript principal (`motor.js:410-418`). É o que o `/context` mostra.
   - **total processado** = soma de tudo, com a conversa relida a cada resposta (`motor.js:522-524`).
   - **material novo** = `input + cache_creation + output` (deixa de fora só a releitura).
7. **Subagentes:** acha a pasta `<sessão>/subagents/` (`motor.js:175-189`), procura arquivo novo a cada 5 voltas (`motor.js:30-31`, `378-379`), liga o gasto ao nome pela linha `tool_result` que traz `agentId: <id>` (`motor.js:291-306`) e pelo `tool_use` de nome `Agent` com `description` (`motor.js:322-328`). Subagente sem nome aparece como "subagente xxxxxx" para a conta fechar (`motor.js:451-457`).
8. **Skills carregadas:** conta os `tool_use` de nome `Skill` (`motor.js:320-321`). Skill não tem gasto separado: ela entra no contexto da conversa.
9. **Nome da sessão:** linhas `custom-title` (o nome que a pessoa deu), depois `ai-title`, depois `last-prompt` cortado em 60 caracteres, e por último os 8 primeiros caracteres do id (`motor.js:278-286` e `467-475`).

**Tabela de preços** (`src/precos.json`, fonte declarada: página de preços da Anthropic, conferida pelo Roger em 02/09/2026), em US$ por milhão de tokens, entrada/saída: Fable 5.1 e Mythos 5.1 10/50 (cache 0,025×); Fable 5 e Mythos 5 10/50; Opus 5, 4.8, 4.7, 4.6 e 4.5 5/25; Sonnet 5 2/10; Sonnet 4.6 e 4.5 3/15; Haiku 4.5 1/5; Haiku 3.5 0,8/4; rápido no Opus 5 e no 4.8 10/50.
**NÃO FOI POSSÍVEL VALIDAR** esses preços contra a página da Anthropic nesta auditoria. A Ponte deve conferir antes de mostrar dinheiro.

> A tabela do Oficina (`oficina-claude/precos.js:20-78`) é **diferente**: não tem a leitura de cache de 0,025× do Fable/Mythos 5.1, nem o custo da busca web, nem Haiku 3.5, Sonnet 4.5 e Opus 4.5. Ali, modelo fora da tabela devolve `null` e a tela mostra tokens sem dinheiro (`precos.js:133-145`). A do painel é a mais completa. Em compensação, o Oficina respeita a variável `CLAUDE_CONFIG_DIR` (`tokens.js:41-45`), e o painel não.

### 2.2 Como sabem que uma conversa está aberta, e se está trabalhando

- **Painel** (`src/vivas.js`): lê cada `~/.claude/sessions/*.json` (`vivas.js:35-70`) e testa se o PID está vivo com `process.kill(pid, 0)` (`vivas.js:22-30`). Processo morto quer dizer "fechada na hora". Processo vivo mas parado há mais de 5 min, a sessão sai da lista (`vivas.js:81-99`, `sessoes.js:16`).
- **Oficina** (`sessaoAtiva.js`): mesma fonte, mais o **`status: 'busy'`** para saber quem está trabalhando (`sessaoAtiva.js:162-172`), e mais o **pai do processo** pelo WMI, para separar as conversas desta janela das de outra (`paisDosProcessos.js:1-20` e `57-76`). Compara o `procStart` do registro com a hora de criação do processo, porque o Windows reaproveita PID (`paisDosProcessos.js:86-107`).

**Para a Ponte, isto é o coração do "avisa quando o trabalho fica pronto":** `status` passando de `busy` para `idle` numa sessão viva quer dizer que o turno acabou.
**NÃO FOI POSSÍVEL VALIDAR** que o `status` muda assim em todas as versões do CLI. O Oficina só anota ter visto `idle` e `busy` (`sessaoAtiva.js:18` e `162`). A conferência é ler esse arquivo na máquina do Alex durante uma conversa.

### 2.3 Como leem o limite de 5 h e de 7 dias

- **Piso, sem rede:** `~/.claude.json` → `cachedUsageUtilization.utilization.five_hour|seven_day` com `utilization` em % e `resets_at` em ISO 8601 (`usoDoPlano.js:1-49` e `81-129`, `limite.js:212-217`). O CLI regrava esse dado de 5 em 5 minutos, e o Roger mediu que ele pode ficar até 50 min parado e errar o limite por 13 a 28 pontos (`faixaDoLimite.js:12-18`).
- **Ao vivo:** o método do SDK `usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET` (`limite.js:64`), chamado numa consulta que **nunca manda mensagem ao modelo** (`consultaDeUso.js:1-45`). Leva 7,4 s para subir e uns 232 MB de memória parada. A cota do servidor aceita 4 chamadas seguidas e recusa a 5ª por 300 s, por isso ele pergunta a cada 5 min (`limite.js` comentário da cadência, `consultaDeUso.js:50`).
- **Empurrado durante a conversa:** a mensagem `rate_limit_event` do SDK (`agente.js:764-765`), em **fração** (0,07) e em segundos desde 1970. Misturar as duas unidades erra por 100× (`limite.js:9-18`).
- Guarda só `five_hour`, `seven_day`, `seven_day_breakdown` e `extra_usage` em `oficina-uso-do-plano.json`, gravado num arquivo ao lado e depois renomeado (`consultaDeUso.js:53-103`). Descarta `accountUuid` (`usoDoPlano.js:38-41`) e o limite `nimbus_quill` ("Weekly Fable"), por ordem do Roger (`usoDoPlano.js:32-49`).
- **Só existe porcentagem.** Os campos de "quanto falta em dinheiro" vêm vazios (`limite.js:20-23`).

---

## 3. A lista completa de funcionalidades (o checklist)

Legenda da coluna **Tauri 2**: **R** é o que vai no Rust, **W** é o que vai na webview. **Reaproveitar** diz se dá para portar o código (MIT, mantendo o aviso) ou só a ideia.

### A. Painel de tokens (`painel-tokens-claude-vscode`)

| # | Funcionalidade | De onde vem o dado | Onde está | Tauri 2 | Reaproveitar |
|---|---|---|---|---|---|
| A1 | Lista de **todas** as conversas abertas, cada uma com o nome | `~/.claude/projects/**.jsonl` + `~/.claude/sessions/*.json` | `sessoes.js:33-110` | R: varrer + `notify` para vigiar a pasta; W: a lista | Portar a lógica para Rust |
| A2 | Custo em US$ equivalente por conversa | `usage` de cada resposta + `precos.json` | `motor.js:60-83` | R: `serde_json` + tabela num `.json` embutido | Portar (MIT) |
| A3 | Contexto agora / total processado / material novo | `usage` da última resposta e soma | `motor.js:410-418`, `sessoes.js:134-147` | R | Portar |
| A4 | Gasto **por subagente**, com o nome da tarefa | `subagents/agent-*.jsonl` + `tool_result agentId` | `motor.js:175-189`, `291-306`, `428-463` | R | Portar |
| A5 | Skills que a conversa carregou (com ×N) | `tool_use name=Skill` | `motor.js:320-321`, `405` | R | Portar |
| A6 | Gasto **por modelo** | `message.model` | `motor.js:508-520` | R | Portar |
| A7 | Modelo desconhecido avisado, em vez de preço calado | prefixos | `motor.js:53-57`, `504` | R | Portar |
| A8 | Leitura incremental e barata (arquivo de 30+ MB não trava) | offset + blocos de 4 MB + orçamento de 250 ms por volta | `motor.js:194-264`, `sessoes.js:20`, `102-109` | R: thread própria (`tokio`), sem orçamento de tempo | Ideia |
| A9 | Sessão some da lista quando o processo morre, ou após 5 min parada | `sessions/<pid>.json` + PID vivo | `vivas.js:22-99` | R: `sysinfo` ou `OpenProcess` | Portar |
| A10 | **Destaca a conversa que está em foco** pelo título da janela do VS Code | título da janela em foco | `sessoes.js:185-228` | R: `GetForegroundWindow` + `GetWindowTextW` | Ideia (na Ponte o foco é a própria Ponte) |
| A11 | Janelinha **sem moldura, transparente, sempre por cima** (nível "screen-saver"), fora da barra de tarefas, que **não rouba o foco** | — | `main.js:154-190` | R: `decorations(false)`, `transparent(true)`, `always_on_top(true)`, `skip_taskbar(true)`, `focused(false)` | Ideia |
| A12 | **Só aparece com o VS Code em foco**, some ao trocar de programa (com 📌 para fixar) | gancho `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` em C# dentro do PowerShell | `main.js:443-470`, `476-558`, `foco.ps1:35-180` | R: crate `windows`, o mesmo gancho, **sem** PowerShell | Ideia (fazer nativo) |
| A13 | Reafirma o "por cima" a cada 2 s (o Windows rebaixa a janela calado) | — | `main.js:422-441` | R: `set_always_on_top` no tique | Ideia |
| A14 | Ícone na bandeja: mostrar, esconder, trazer de volta, sempre visível, fechar | — | `main.js:338-420` | R: `tauri::tray` | Ideia |
| A15 | Lembra a posição e **devolve para dentro da tela** se o monitor sumir | `posicao.json` em `userData` | `main.js:117-152`, `291-300`, `782-792` | R: `tauri-plugin-window-state` + checagem própria | Ideia |
| A16 | Faixa rasa de 34 px que **abre para baixo** com o detalhe e cresce para a esquerda | IPC `expandir` / `largura` | `main.js:812-865` | R: `set_size`/`set_position`; W: mede e pede | Ideia |
| A17 | Uma instância só; abrir de novo traz de volta; `--fechar` fecha de fora | — | `main.js:662-711` | R: `tauri-plugin-single-instance` | Ideia |
| A18 | Volta do repouso, troca de monitor e GPU caída repõem a janela | `powerMonitor`, `screen` | `main.js:755-792`, `879-887` | R: eventos do Windows (`WM_POWERBROADCAST`, `WM_DISPLAYCHANGE`) | Ideia |
| A19 | Registro em disco com teto de 200 KB, sem conteúdo da conversa | `registro.txt` | `main.js:42-109` | R: `tracing` + rotação | Ideia |
| A20 | Dica do mouse com tudo por extenso ("a conversa relida ~N vezes") | dados de A3 | `index.html:280-300` | W | Copiar o texto e a ideia |
| A21 | Modo foto (abre, fotografa a janela, sai), para conferir o desenho | `MEDIDOR_FOTO` | `main.js:630-660` | R: `WebviewWindow` + captura | Ideia (bom para o eng-qa) |
| A22 | Conferência independente do cálculo em Python | `testes/conferir.py` | `conferir.py` | — | Ideia: teste com fixture |

### B. Conversa com o Claude (`oficina/extensoes/oficina-claude`)

| # | Funcionalidade | De onde vem o dado | Onde está | Tauri 2 | Reaproveitar |
|---|---|---|---|---|---|
| B1 | Conversa própria com **resposta aparecendo enquanto é gerada** | SDK `query({prompt: fila, includePartialMessages: true})`, eventos `stream_event` → `text_delta` e `thinking_delta` | `agente.js:322-402`, `827-833` | Ver §4.2 | Ideia |
| B2 | Estados da conversa: parada, abrindo, ociosa, pensando, esperando permissão, cancelando, erro | laço de mensagens | `agente.js:51-59` | R: enum + evento para W | Ideia |
| B3 | **Sinal de "terminou"**: mensagem `result` (com `total_cost_usd`) | SDK | `agente.js:861-862` | R | Ideia |
| B4 | **Pedido de permissão em cartão**, com frase em português ("Escrever o arquivo X", "Rodar um comando: …") | `canUseTool(nome, entrada, opções)` | `agente.js:137-142`, `401`, `986` | R: recebe o pedido; W: cartão | Copiar as frases |
| B5 | Todo comando passa pela porta, inclusive `echo` | `settings.permissions.ask` com as ferramentas de comando | `agente.js:363-381` | R | Ideia |
| B6 | Regras da pasta valem dentro (`deny`, hooks) | `settingSources: ['user','project','local']` | `agente.js:324-350` | R | Ideia |
| B7 | Modos `default`, `acceptEdits`, `plan`, `bypassPermissions` (este só com chave ligada de propósito) | `setPermissionMode` | `agente.js:1190-1264` | R | Ideia |
| B8 | **Parar** sem perder a conversa (`interrupt`, não `abort`) | SDK | `agente.js:1306-1335` | R: mandar o controle de interrupção | Ideia |
| B9 | **Trocar modelo e esforço no meio** e avisar quanto custa reescrever o cache | `setModel`, `applyFlagSettings({effortLevel})`, `supportedModels` | `agente.js:525-575`, `modelos.js:126-171` | R + W | Copiar a conta de `modelos.js:126` |
| B10 | Quem está logado (e-mail, plano) | `accountInfo()` | `agente.js:604-612` | R | Ideia |
| B11 | Lista de comandos `/` e skills da pasta | `initializationResult().commands` e `commands_changed` | `agente.js:428-463`, `821-822` | R | Ideia |
| B12 | Retomar e bifurcar conversa | `resume`, `forkSession` | `agente.js:361-362` | R | Ideia |
| B13 | Revisão de mudança de arquivo em diff, aceitar ou rejeitar **por trecho**; nunca sobrescreve arquivo não salvo; recusa binário e não-UTF-8 | `proposta.js` simula o Write/Edit | `proposta.js:62-141`, `revisao.js:71` | W: um diff (Monaco ou `diff2html`); R: gravação atômica | Portar `proposta.js` (puro, MIT) |
| B14 | Comando aprovado roda **à vista num terminal**, e a saída volta ao agente | `comando.js` acha o PowerShell do System32 e o bash do Git (evita o do WSL) | `comando.js:80-109`, `136-185`, `224` | R: `portable-pty`; W: `xterm.js` | Ideia |
| B15 | Parar mata **só aquele comando e os filhos**, pelo PID, nunca pelo nome | `taskkill /PID … /T /F` | `comando.js:352`, `408` | R: Job Object do Windows (mais seguro que `taskkill`) | Ideia |
| B16 | Fechar a janela encerra o que o agente deixou rodando | tabela de processos (WMI) | `encerramento.js:42-117` | R: Job Object com `KILL_ON_JOB_CLOSE` | Ideia |
| B17 | Explicar, Corrigir, Gerar teste, Perguntar sobre o trecho selecionado | texto montado com cerca segura | `acoes.js:83-191` | W | Ideia (a Ponte não é editor) |

### C. Agentes (Oficina)

| # | Funcionalidade | De onde vem o dado | Onde está | Tauri 2 | Reaproveitar |
|---|---|---|---|---|---|
| C1 | Botão **"N agentes rodando"** | eventos `task_started`, `task_progress`, `task_updated`, `task_notification`, `background_tasks_changed` (só `task_type: 'local_agent'`) | `agentes.js:1-45`, `agente.js:772-776` | R | Portar `agentes.js` (puro) |
| C2 | **Mapa dos agentes** em árvore: estado (rodando, em pausa, terminou, falhou, parado), tempo, tokens, ferramentas, o que está fazendo agora, resumo do fim | ao vivo pelos eventos; pai pelo `parent_tool_use_id` (2º nível vem com `spawn_depth: 2` e sem pai) | `agentes.js:13-25`, `mapaDosAgentes.js:96-140` | W: SVG ou HTML; R: árvore | Portar |
| C3 | Mapa de conversa **retomada ou de outro programa**, lido do disco | `agent-<id>.meta.json` (`description`, `agentType`, `toolUseId`) | `tokens.js:68-85`, `227` | R | Portar |
| C4 | **Parar um agente** pelo cartão, com pergunta antes (o foco fica em "Deixar rodando") | SDK | `agente.js:656-690` | R | Ideia |
| C5 | Resumo de progresso do agente **desligado de propósito** (custaria ~US$ 9/h por agente) | `agentProgressSummaries: false` | `agente.js:383-392` | R | Copiar a decisão |

> Limite declarado pelo próprio Roger (CHANGELOG v30): o mapa lido do disco mostra a árvore e o estado, mas **não o que cada agente faz neste instante**. Isso só existe na conversa que o próprio programa abriu.

### D. Tokens e cache (Oficina, além do painel)

| # | Funcionalidade | De onde vem o dado | Onde está | Tauri 2 | Reaproveitar |
|---|---|---|---|---|---|
| D1 | **Relógio do cache**: anel com os minutos até o cache esfriar; vencido, avisa que a próxima mensagem sai cara | TTL lido de `ephemeral_1h_input_tokens` / `ephemeral_5m_input_tokens`; começa no `timestamp` da última resposta; vale o menor quando há os dois; sem dado, 60 min "suposto" | `relogioCache.js:1-117` | R: cálculo; W: anel | Portar (puro) |
| D2 | Conversa achada **pelo id**, não por "a mais recente da pasta" | `sessions/<pid>.json` → `sessionId` | `sessaoAtiva.js:1-30`, `tokens.js:46-57` | R | Portar |
| D3 | Só as conversas **desta janela** | pai do processo pelo WMI | `paisDosProcessos.js` | R: `sysinfo` dá o pai sem WMI | Ideia |
| D4 | Linha que **encolhe em degraus** quando falta espaço (sai o total, depois a soma, depois encurta o nome), medindo de verdade | — | `mostradorDeTokens.js:213-254` | W | Portar |
| D5 | Três estados honestos: número, "— 0" sem conversa, "— ?" quando a medição falhou | — | CHANGELOG V24 | W | Copiar a regra |
| D6 | Skill vista também pelo `<command-name>/nome</command-name>` (skill chamada pela barra) | transcript | `tokens.js:168-177` | R | Portar (o painel não tem isto) |

### E. Limite do plano, conta e conexões (Oficina)

| # | Funcionalidade | De onde vem o dado | Onde está | Tauri 2 | Reaproveitar |
|---|---|---|---|---|---|
| E1 | Barras **5h** e **7d** com %, sempre na tela; `!` a partir de 90%; dica com a hora em que zera | §2.3 | `usoDoPlano.js`, `faixaDoLimite.js`, `limite.js` | R: lê `~/.claude.json` a cada 5 min + `notify`; W: barras | Portar a tradução |
| E2 | "**Perder nunca**": falha nenhuma apaga o último número | — | `faixaDoLimite.js:24-27` | R | Copiar a regra |
| E3 | Tela do consumo (detalhe por janela, quanto falta para zerar) | registro + consulta | `telaDoConsumo.js:27-131` | W | Portar (já escapa HTML, CSP `default-src 'none'`) |
| E4 | Tela da **conta**: e-mail, plano, como entrou; botão sair/entrar | `claude auth status --json`, `claude auth logout`, `claude auth login` | `conta.js:157-172` | R: `Command` sem shell | Ideia |
| E5 | Tela das **conexões MCP**: quem respondeu, quem espera login, quem falhou e por quê; botão Entrar | `claude mcp list`, `claude mcp get <nome>`, `claude mcp login <nome>` | `mcps.js:238-336`, `383-436` | R: `Command` + parser de texto | Portar o parser (frágil: é texto) |

### F. Sessões antigas, nome e pasta (Oficina)

| # | Funcionalidade | De onde vem o dado | Onde está | Tauri 2 | Reaproveitar |
|---|---|---|---|---|---|
| F1 | **Conversas de outros dias** da pasta: modelo, custo, tamanho; retomar, renomear, etiquetar, procurar por texto | SDK `listSessions`, `getSessionMessages`, `renameSession`, `tagSession` (atenção: `listSessions` devolve zero, calado, com barra invertida no caminho) | `sessoes.js:1-30`, `161-207` | R: ler os `.jsonl` direto; renomear gravando a linha `custom-title` (**a validar**) | Ideia |
| F2 | Nome da conversa | linha `custom-title` lida da cauda do transcript | `tituloDaConversa.js:41-99` | R | Portar |
| F3 | Renomear pelo botão direito na aba | — | `renomearConversa.js:42` | W | Ideia |
| F4 | **Nunca conversar sem pasta** (sem pasta não valem `CLAUDE.md` nem skills); "pasta de sempre"; aviso `⚠ sem pasta` | config `oficina.pastaDeSempre` | `pastaDeSempre.js:41-132` | R + W | Copiar a regra |
| F5 | Skills: clicar manda `/nome`, estrela, olho, arrastar; nome com espaço ou símbolo não entra | comandos do SDK | `skills.js:44-146` | W | Portar |
| F6 | Socorro: reabrir conversa, abrir registro, recarregar | — | CHANGELOG V8 | R + W | Ideia |
| F7 | Registro sem a pasta pessoal nos caminhos | — | `registro.js:42-67` | R | Copiar a regra |

### G. O que é do editor e a Ponte provavelmente não precisa

Navegador com tamanhos de aparelho (`navegador.js:63-125`), Ver a página / Ver o código (`verHtml.js`), layouts com nome (`layout.js:141`), alça da caixa de escrever (`tamanhos.js`), Conectar ao GitHub (`github.js:66`), leitores de PDF/Office, dois temas medidos por ΔL* ≥ 3 (`identidade/gerar_temas.py`) e os 30 patches do núcleo do VS Code.
**Não reaproveitar:** `ajustesDaConversaOficial.js`. Ele **escreve CSS dentro dos arquivos da extensão oficial do Claude Code** (proprietária), o que só faz sentido dentro de um VS Code.

### H. Boneco 3D da Sara (`blender-3d-web`, só referência)

Guia, não código: Blender → glTF/GLB; o que sobrevive à exportação; compressão Draco ou Meshopt para a malha e KTX2/Basis para a textura; orçamento de polígonos e de peso; exibir com react-three-fiber ou `<model-viewer>`; Blender MCP (`ahujasid/blender-mcp`, que o próprio guia marca como **execução de código arbitrário** e avisa que o "mirror" não é fonte confiável); onde achar assets prontos. Seções em `README.md:15-135`. Sem licença: ler e aplicar, não copiar o texto.
Na Ponte, o retrato fica na webview com three.js, num GLB leve. Se o Alex quiser o Blender MCP, ele passa pelo portão antes.

---

## 4. Como isso se encaixa num app Tauri 2

### 4.1 Divisão de trabalho

- **Rust (núcleo):**
  - leitor incremental de `.jsonl` (A2–A8, D1, D6) com `serde_json`;
  - vigia de pastas com `notify` em `~/.claude/projects` e `~/.claude/sessions`;
  - teste de PID vivo e pai do processo com `sysinfo`;
  - leitura de `~/.claude.json` (E1);
  - chamadas a `claude auth status --json` e `claude mcp list` por `std::process::Command`, **sem shell**;
  - gancho de foco e "sempre por cima" com o crate `windows`;
  - bandeja, instância única, posição da janela, aviso do Windows com `tauri-plugin-notification`.
- **Webview:** retrato da Sara (three.js), lista de conversas, mapa dos agentes, barras de limite, cartões de permissão, terminal (`xterm.js`) e diff.
- **Duas janelas:** a principal e a **miniatura sempre no canto** (A11–A18). É nela que o "pronto" vira aviso.

### 4.2 Conversar com o Claude: a decisão que falta

O Oficina conversa pelo **SDK de TypeScript** (`@anthropic-ai/claude-agent-sdk` 0.3.261, `package.json:68-70`, integridade sha512 no lockfile). O Tauri não roda Node por dentro. Há duas rotas, e **as duas pedem validação antes de virar plano**:

1. **Sidecar Node com o SDK.** É o caminho já provado pelo Oficina. Custa um Node embutido e uns 230 MB por processo. A licença do SDK é `SEE LICENSE IN README.md`, ou seja, termos da Anthropic, não MIT. **NÃO FOI POSSÍVEL VALIDAR** se ele pode ir embutido num instalador.
2. **Rust conversando direto com o `claude` CLI** em modo de entrada e saída `stream-json`, que é o que o SDK faz por baixo. Não precisa de Node. **NÃO FOI POSSÍVEL VALIDAR** que permissão por cartão (o `canUseTool`) e o resto do protocolo de controle funcionam sem o SDK. Isso tem que ser medido num protótipo.

Antes de escolher, peça ao `claude-code-guide` ou à skill `claude-api` o estado atual do SDK e das flags do CLI.

### 4.3 O aviso de "pronto", que nenhum dos dois faz completo

- **Conversa aberta pela Ponte:** a mensagem `result` (B3).
- **Conversa aberta no terminal ou no VS Code:** `status` `busy` → `idle` no `sessions/<pid>.json` (§2.2), ou um **hook `Stop`/`Notification`** do Claude Code que avisa a Ponte. O Roger já usa hooks `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse` e `Stop` (`claude-pombo/compartilhado/mestre-roger/settings_hooks.json`), o que prova que o mecanismo existe. Instalar hook no `~/.claude/settings.json` do Alex **muda a configuração dele** e precisa do "sim" dele.

### 4.4 Licença: o que dá para copiar

- `painel-tokens-claude-vscode` e `oficina` são **MIT**. Pode portar e copiar, desde que o aviso de copyright e a licença vão junto: um `NOTICE.md` na Ponte com "Copyright (c) 2026 Roger Fleming" e "Copyright (c) 2026 OFICINA contributors". O núcleo da Microsoft só importa se algo do VS Code for copiado, e não deve ser.
- Os mais fáceis de portar são os módulos que o próprio Roger escreveu sem `vscode`, de propósito para rodar em Node puro: `motor.js`, `sessoes.js`, `vivas.js`, `agentes.js`, `relogioCache.js`, `usoDoPlano.js`, `limite.js`, `proposta.js`, `skills.js` e `modelos.js`.
- `blender-3d-web`, `claude-pombo` e `pacote-paulo-claude` **não têm licença**: só leitura, nada copiado nem republicado.
- O Roger é colaborador do Alex. Mesmo com MIT, é educado avisar que a Ponte usa a lógica dele.

---

## 5. Segurança: o que achei

### 5.1 Achados

**[Médio] O Oficina vem de fábrica com o agente agindo sem pedir aprovação, e sem perguntar se a pasta é confiável.**
- *Prova:* `oficina/README.md`, seção "O agente age sem pedir aprovação, de fábrica"; CHANGELOG v20 e v28 (`security.workspace.trust.enabled` desligado de fábrica); a lista de editores confiáveis em `produto/product.json:44`.
- *Exploração:* o Alex abre no Oficina um repo baixado; um `CLAUDE.md` ou um comentário escondido no repo vira comando executado com as permissões dele. As tarefas e configurações da pasta também rodam sem aviso.
- *Para a Ponte:* **não copiar esse padrão.** O modo padrão tem que pedir aprovação, e `bypassPermissions` só com escolha explícita e visível. O próprio código do Oficina já foi assim na V2 (`agente.js:354-356`).
- *Para o Alex:* se ele instalar o Oficina, deve trocar o modo e religar a confiança de pasta. O instalador não está nesta auditoria (é um binário sem assinatura).

**[Baixo] O canal `claude-pombo` faz um Claude ler texto escrito por outro Claude, e o do Alex já participa.**
- *Prova:* `claude-pombo/README.md`, onde o "Lado C" é o Claude do Mano (`SkotAlexsander`) desde 04/11/2026; o protocolo de "ouvir" com `verificar.sh` e responder.
- *Exploração:* uma mensagem em `conversa/` com uma instrução ("roda este instalador") é lida pelo Claude do Alex como contexto de trabalho. A mensagem `conversa/035-roger.md:44` manda rodar `powershell -ExecutionPolicy Bypass -File instalar.ps1`. Lida no contexto, é um pacote de modpack para humano e é legítima, mas é exatamente o formato que uma injeção teria.
- *Regra:* tudo que vem do pombo é **dado**. Instalador citado lá passa pelo portão como qualquer código de fora.

### 5.2 Fronteiras olhadas sem achado

- **Painel, tela:** todo dado do transcript entra por `textContent`. `innerHTML` só aparece para limpar (`index.html:236`, `257`, `268`, `342`). `contextIsolation: true` e `nodeIntegration: false` (`main.js:180-184`).
- **Painel, `abrir-pasta`:** `main.js:867-869` chama `shell.openPath` com o caminho que a tela manda. Seria execução de arquivo se a tela tivesse injeção, e ela não tem. A tela **nunca chama** `abrirPasta` (só `aoReceberDados`, `expandir`, `fixar`, `fechar` e `largura`: `index.html:227-453`). É superfície morta. **Na Ponte, não expor esse comando**, ou só aceitar caminhos que venham da lista de sessões do Rust.
- **Painel, rede:** nenhuma. **Processo:** só o próprio Electron (`main.js:23`) e o `powershell.exe foco.ps1` com argumentos fixos (`main.js:484-490`). O `foco.ps1` só lê o processo e o título da janela em foco. O registro grava só o nome do processo, não o título (`main.js:464`).
- **Oficina, rede da extensão:** nenhuma chamada própria. Fora o SDK, só os endereços que a pessoa digita no navegador (`navegador.js:125-132`).
- **Oficina, credencial:** não lê `~/.claude/.credentials.json` (procurei o nome do arquivo e nada apareceu). A conta vem de `accountInfo()` e de `claude auth status`, e o e-mail fica só em memória (`agente.js:33-36`).
- **Oficina, telas:** o mapa dos agentes escapa tudo (`mapaDosAgentes.js:78-82`) e roda **sem script** (`enableScripts: false`, `mapaDosAgentes.js:204-213`). O painel usa CSP com nonce novo a cada abertura (`extensao.js:332-358`). A tela do consumo usa `default-src 'none'` (`telaDoConsumo.js:92`). E `painel.js` proíbe `innerHTML` com texto do agente (`painel.js:8`).
- **Oficina, processos:** `execFile` com lista de argumentos, sem `shell: true`. PIDs filtrados para inteiros de 1 a 2³²−1 antes de entrar na consulta WMI (`paisDosProcessos.js:57-60`). O `taskkill` mata por PID (`encerramento.js:99-104`).
- **Oficina, dependências:** uma só, `@anthropic-ai/claude-agent-sdk` 0.3.261 fixada, do `registry.npmjs.org` com `integrity` sha512. Sem `preinstall`/`postinstall` em nenhum `package.json`.
- **Oficina, CI:** o workflow `subir_upstream.yml` usa actions fixadas por SHA e não roda por `pull_request`/`pull_request_target`, ou seja, terceiro nenhum aciona.
- **Segredos:** nenhum token ou chave encontrado no que foi lido. O `daedalus-status` usa `secrets.DAEDALUS_EMERGENCIA_JWK` pelo GitHub Secrets, e o valor não está no repo.

### 5.3 Notas menores (endurecimento, não falha)

- `encerramento.js:86` chama `powershell.exe` e `taskkill` **pelo nome**, enquanto `paisDosProcessos.js:74` usa o caminho do System32. Um `powershell.exe` posto na frente do PATH rodaria no lugar. Na Ponte: caminho absoluto sempre.
- O painel não tem CSP no `index.html`. Na Ponte, CSP estrita no `tauri.conf.json`.
- As extensões do Oficina vêm da Open VSX como `latest`, sem versão nem hash fixados (`scripts/baixar_extensoes.mjs:41-49`, `extensoesQueFaltam.js:90`), e o publicador `anthropic` está na lista dos confiáveis (`product.json:44`).

### 5.4 A conferir (sem severidade)

- **Quem publica `anthropic.claude-code` na Open VSX.** Não verifiquei se o namespace é da Anthropic. Resolve olhando o namespace verificado na Open VSX. Só importa se o Alex for instalar o Oficina.
- **O `status` `busy`/`idle` de `sessions/<pid>.json`** como sinal de "pronto" (§2.2). Resolve lendo o arquivo durante uma conversa real.
- **Os preços da tabela** (§2.1). Resolve comparando com a página de preços da Anthropic.

### 5.5 Candidatos descartados (falso positivo do scanner)

- Oficina, CRÍTICO DESTR-003 em `scripts/empacotar.bat:114`: é `del /f /q "%ALVO%"` de **um arquivo gerado** (`%DIST%\%NOMEEXE%`), não de um disco.
- Oficina e painel, OCULTO-001 (caractere invisível): é o BOM U+FEFF **dentro de uma regex que remove BOM** (`proposta.js:16`) e o BOM que o painel põe no começo do log para o Bloco de Notas ler acentos (`main.js:106`).
- Oficina e painel, EVASAO-002 (`-ExecutionPolicy Bypass`): em comentários de "como rodar" de testes, e no `foco.ps1` com argumentos fixos.
- Oficina, REDE-001 (IP cru): `127.0.0.1` em testes e no navegador local.
- pacote-paulo, CRÍTICO EXEC-001 (`curl … | sh`): é o instalador oficial do `uv` (`astral.sh`), citado no README e no `instalar.sh`. Continua sendo "baixa e executa", então não rodar.
- pacote-paulo, ALTO OFUSC-004 (base64 de 64 KB): é o `llhttp` em WebAssembly do `undici`, empacotado num `server.mjs` gerado por bundler (`server.mjs:3029-3042`).
- pacote-paulo, INJ-001, INJ-003 e EXFIL-001: frases de um **checklist de segurança** e de uma fonte citada, não ordens.
- pacote-paulo, `CLAUDE.md` (neutralizado): manda o agente **oferecer** rodar um instalador de mais de 1 GB logo na abertura, com confirmação do usuário. Não é malicioso, mas é instrução de agente que vem dentro de um repo, que é o motivo de o passo de neutralizar existir.

### 5.6 O que não foi testado

- Nada foi executado: nenhum teste, nenhum build, nenhum binário. O instalador `OficinaSetup.exe` das Releases não foi baixado.
- O núcleo do VS Code com os 30 patches não foi auditado; só os `.md` dos patches foram vistos por cima.
- `pacote-paulo-claude` e `claude-pombo` foram triados, não lidos linha a linha.

---

## 6. Como foi feito

- `gh repo list rogeriofleming --limit 100`: 14 repos, 6 escolhidos.
- Clone raso com `core.symlinks=false`, `core.hooksPath=/dev/null` e sem submódulos, em `…\scratchpad\quarentena-rogerio\`.
- Instruções para agente lidas como dado e renomeadas com `.QUARENTENA`: `claude-pombo/compartilhado/mestre-roger/CLAUDE.md` e `pacote-paulo-claude/CLAUDE.md`.
- Scanner `verificar_seguranca.py` (com `python -I`): painel exit 1, oficina **exit 2**, pombo exit 1, pacote-paulo **exit 2**, blender e daedalus-status exit 0. Cada CRÍTICO e ALTO foi aberto e conferido (§5.5).
- Leitura inteira: `motor.js`, `sessoes.js`, `vivas.js`, `preload.js`, `main.js`, `foco.ps1` e `precos.json` do painel; README e CHANGELOG do Oficina. Leitura dirigida, por cabeçalho, função e busca, dos 45 módulos de `oficina-claude`.
- Achado refutado antes de virar achado: o `abrir-pasta` do painel (§5.2).
