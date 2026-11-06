# Relatório de QA 01 · Ponte de Comando · Sara

**Quem:** Tomás (`eng-qa`) · **Data:** 06/11/2026, 00h40–02h20 · **Pedido:** autoanálise de bugs, achar e provar, sem corrigir.

**O que foi testado:** o commit `8887de6` e o `target\debug\ponte-de-comando.exe` de 01:20, que estava em dia com esse commit. Um teste também rodou no `target\release` de 01:29.

> **Atenção, o código mudou durante o teste.** Às 02:02 entrou o commit `2298fb6` (4 linhas em `ui/ponte-main.js`). Desde 02:08 há trabalho não commitado de outro agente: `cofre.rs`, `preferencias.rs`, +410 linhas em `lib.rs` (conta, chave de API, iniciar com o Windows). **Nada disso foi testado aqui.** Os números de linha abaixo são do commit `8887de6`. O teste de abas (B7) rodou também com o `ponte-main.js` novo, e o bug continua lá.

Todos os testes de programa rodaram **isolados e fora da tela**: dados, WebView2, pasta de trabalho e porta próprios em `%TEMP%\ponte-qa-*`, já apagados. Nenhum teste tocou o `%APPDATA%` do programa instalado, a config real, o `settings.json` nem o `valores.exe`. As sessões reais do `~/.claude` só foram **lidas**: pelo motor, como o programa faz, e por uma contagem de linhas e bytes sem ler conteúdo. Chamadas reais ao Claude: 3 conversas com Haiku, esforço baixo, 7 turnos curtos no total (programa: 2, job: 1, QA: 4).

---

## 1. Placar

### Suítes da casa

| Suíte | Esperado | Obtido |
|---|---|---|
| `npm run testar`: hook | 7/7 | **7/7** |
| `npm run testar`: cargo | 26 ok + 1 ignorado | **25 ok, 1 FALHOU, 1 ignorado** na 1ª rodada (`servidor::testes::token_e_origem`, `ConnectionReset 10054`). É **intermitente**: falhou em 2 de 16 rodadas e passou 8 de 8 sozinho. Causa no bug B10. |
| `npm run testar:paginas` | 14/14 | **14/14** |
| `npm run testar:programa` | 32/32 | **32/32** |
| `npm run testar:job` | OK | **OK** |
| `node ferramentas\testes\miniatura.mjs` (não está no `package.json`) | 12/12 | **12/12** |

### Testes novos (`ferramentas\testes\qa\`)

Cada `OK` é o comportamento esperado. Cada `FALHOU` é um bug provado.

| Arquivo | O que cobre | Placar |
|---|---|---|
| `comum.mjs` | ajudante: abre o exe isolado e fora da tela, mede CPU e memória, limpa com caminho absoluto certo | — |
| `servidor.mjs` | JSON quebrado, corpo e cabeçalho gigantes, token errado, Origin, conexões lentas, hook com stdin ruim, porta ocupada | 12/17 |
| `arquivos-config.mjs` | config.json corrompido, pasta sumida e sem permissão, junção, acento, emoji e nomes reservados, rede e dispositivo, barreiras dos comandos, miniatura com monitor inexistente | 7/18 |
| `tokens.mjs [MB]` | `~/.claude` falso: transcript gigante com linha truncada, `.claude.json` sem `cachedUsageUtilization` | 3/4 |
| `memoria-motor.mjs [s]` | pico de memória do exe lendo as sessões vivas reais | 0/1 |
| `conversa.mjs` | real (Haiku): mandar com a conversa respondendo, PARAR, permissão aprovada, sidecar morto com o cartão aberto, retomar | 14/16 |
| `ciclo.mjs` | fechar no carregamento, instância única, rajada de 20, CPU, vazamento, carga da Main, conta e conexões | 5/8 |
| `cpu.mjs [--claude-vazio]` | CPU por processo com tudo escondido | 0/1 |
| `visibilidade.mjs` | as páginas escondidas sabem que estão escondidas? | 0/2 |
| `vazamento.mjs [trocas]` | memória a cada 20 trocas Main ⇄ Fluxo | 1/1 |
| `abas.mjs [ponte-main.js]` | duas abas conversando ao mesmo tempo (navegador comum, sem gasto) | 1/3 |
| `duas-sessoes.mjs` | o aviso de pronto de uma conversa sobrevive à ferramenta de outra? | 0/1 |

### O que passou nos testes novos

- Instância única: a segunda abertura sai em 0,2 s e traz a primeira.
- Fechar no meio do carregamento: a principal abre mesmo assim (8,7 s).
- Rajada de 20 minimizar/restaurar sem pausa termina no estado certo.
- Servidor dos hooks:
  - escuta **só em 127.0.0.1**;
  - JSON quebrado → 400, tipo errado → 400, sem Content-Length → 400;
  - cabeçalho de 10 KB → 431;
  - token errado → 401, Origin → 403;
  - o hook com stdin quebrado ou de 5 MB sai com 0 em ~0,2 s.
- Barreiras de arquivo:
  - junção para fora: listar, ler e criar recusados;
  - `\\127.0.0.1\C$` e `\\.\C:` recusados;
  - acento e emoji criam certo.
- `config.json` que nem é JSON: o programa abre.
- Miniatura com monitor gravado inexistente e canto inválido: abre no canto padrão do monitor principal.
- Transcript com linha truncada no meio e no fim: conta as 1150 respostas certas, nenhuma inventada.
- `~/.claude.json` sem `cachedUsageUtilization` → `{"disponivel":false}`, sem quebrar.
- Conversa real:
  - mandar com a conversa respondendo é recusado com mensagem clara;
  - PARAR emite `cancelado`;
  - permitir cria o arquivo e termina em "pronto";
  - com o sidecar morto, o cartão some e chega o erro;
  - o `claude.exe` órfão morre;
  - responder o cartão velho dá "Esse pedido já foi respondido ou cancelado.";
  - a próxima mensagem abre outro sidecar e **retoma a mesma sessão**: o Claude lembrou do `morto.txt`.
- 80 trocas Main ⇄ Fluxo: a memória estabiliza (684 → 830 → 818 → 840 → 814 MB) e o número de processos não cresce. **Não há vazamento por troca de página.**
- `conta()` e `conexoes()` respondem (12,8 s, 12 conexões) e não deixam processo filho para trás.

---

## 2. Bugs por gravidade

| # | Gravidade | Título |
|---|---|---|
| C1 | **crítico** | Com as janelas escondidas, o programa segue desenhando a 145 quadros/s: ~2,9 núcleos de CPU/GPU com tudo na bandeja |
| A1 | **alto** | PARAR vira "Alex, travei numa parte." (estado erro, som e notificação) |
| A2 | **alto** | Ir ao FLUXO e voltar, ou reabrir o programa, troca a conversa da aba por uma sessão NOVA do Claude |
| A3 | **alto** | Motor de tokens: a 1ª leitura das sessões reais leva de 40 s a ~2 min, o exe chega a 923 MB e não devolve; leitura quadrática por bloco |
| M1 | médio | Sidecar morto: a Sara fica presa em "Alex, preciso de você." |
| M2 | médio | Duas abas na Main: quando uma termina, a outra perde o "trabalhando" e o PARAR |
| M3 | médio | O "pronto" de uma conversa é apagado quando outra conversa usa uma ferramenta |
| M4 | médio | Um campo de tipo errado no `config.json` zera a config inteira, e o próximo clique grava por cima |
| M5 | médio | Porta ocupada: nenhum aviso, o arquivo `porta` velho fica, e o token do hook vai para o outro programa |
| M6 | médio (seg.) | `conversas_antigas` aceita qualquer pasta, fora da pasta de trabalho |
| M7 | médio (seg.) | A própria página muda a barreira: `config.gravar({pastaTrabalho})` aceita `C:\Windows` e `"."` |
| M8 | médio (seg.) | Sem CSP, com fontes da internet e `withGlobalTauri`: qualquer script injetado usa todos os comandos |
| B1 | baixo | Pasta de trabalho gravada que sumiu: cai calada no `A:\Claude` |
| B2 | baixo | Pasta sem permissão: a mensagem diz "Essa pasta não existe: \\?\C:\…" |
| B3 | baixo | Nomes reservados aceitos: `COM¹.txt`, `CONOUT$`, `NUL .txt` |
| B4 | baixo | 16 conexões lentas calam o servidor dos hooks |
| B5 | baixo | Resposta 413 perdida por RST; mesma classe do teste do cargo que falha às vezes |
| B6 | baixo (seg.) | Comandos expostos que nenhuma tela usa (`mini_soltou`, `parar_agente`, `conversas_antigas`, `retomar_sessao`, `conta`, `conexoes_mcp`, `limites`, `config_gravar`) |
| B7 | baixo (seg.) | O build release obedece `PONTE_DEPURAR_PORTA`, `PONTE_SIDECAR`, `PONTE_NODE`, `PONTE_CLAUDE`(+`_PREFIXO`) |
| B8 | baixo (seg.) | Processos filhos: `claude` sem caminho absoluto no plano B, `node` pelo PATH, stderr não lido, fora do Job |
| B9 | baixo (teste) | `limpar.mjs` nunca mata nada: `\t` e `\v` dentro dos caminhos |
| B10 | baixo (teste) | `testar:programa` tem um teste que passa sem testar ("silenciar… é gravado") e um caminho que não limpa se o exe cair |
| B11 | baixo (teste) | A instância única é a do programa instalado: um teste com a Ponte instalada aberta foca a janela do Alex |

---

### C1 · crítico · Com as janelas escondidas, o programa segue desenhando a 145 quadros/s

- **Reproduzir:** `node ferramentas\testes\qa\cpu.mjs --claude-vazio` e `node ferramentas\testes\qa\visibilidade.mjs`. Ambos usam um `~/.claude` falso e vazio, para o motor de tokens não entrar na conta.
- **Esperado:** principal na bandeja e miniatura desligada → `document.visibilityState === 'hidden'`, vídeos e animações parados, CPU perto de 0.
- **Obtido:**
  ```
  escondido: main {"vis":"visible","hidden":false,"quadrosPorS":145,"videosTocando":2,"animacoes":10}
           · mini {"vis":"visible","hidden":false,"quadrosPorS":145,"videosTocando":2,"animacoes":0}
  tudo escondido: 286.6 % de um núcleo
    msedgewebview2 gpu-process: 187.0 % · 477 MB
    msedgewebview2 renderer:     88.0 % · 108 MB
  principal na tela: 347.0 % de um núcleo · só a miniatura (ciclo.mjs): 313 %
  ```
- **Arquivo:linha provável:**
  - `lib.rs:1045-1050` e `lib.rs:356`: `hide()` esconde a janela, mas o WebView2 continua "visível" para a página;
  - `ui/mini.html`: os dois `<video autoplay loop>` e o `ResizeObserver`, sem `visibilitychange`;
  - `ui/estudio/Main.dc.html`: 2 vídeos e 10 animações, também sem pausa.
- **Observação:** o programa fica o dia todo na bandeja. Isso gasta ~3 núcleos o tempo todo, num PC em que a RAM já é o gargalo. Mesmo na tela, 3,5 núcleos para a Main é muito. **Não validado:** a medida com as janelas na posição real (−30000 nos testes), porque eu não podia pôr janela na tela do Alex. A medida "escondido", que é a que importa, não depende da posição.

### A1 · alto · PARAR vira "Alex, travei numa parte."

- **Reproduzir:** `node ferramentas\testes\qa\conversa.mjs`, turno 1: "Conte de 1 a 120", PARAR no primeiro pedaço da resposta.
- **Esperado:** "cancelado", a Sara volta a "esperando", sem aviso de erro.
- **Obtido:** a ordem dos eventos é `cancelado, texto, resultado`. O `resultado` vem com `erro: true` e vira:
  ```
  estado=erro · aviso=Alex, travei numa parte. · trabalho: Algo deu errado.
  ```
  O aviso de erro toca som e, fora da tela, manda notificação do Windows. Na Main, o adaptador ainda acrescenta "Parei com erro: …" depois de "Parei aqui".
- **Arquivo:linha provável:**
  - `sidecar/ponte-sidecar.mjs:177-184`: manda `cancelado` e depois deixa passar o `result` do turno interrompido;
  - `lib.rs:676-682`: trata todo `resultado` com erro como "erro";
  - `ui/ponte-main.js:108-113`: idem na tela.

### A2 · alto · Ir ao FLUXO e voltar, ou reabrir o programa, troca a conversa da aba por uma sessão NOVA

- **Reproduzir:** `node ferramentas\testes\qa\ciclo.mjs` (sem gasto).
- **Esperado:** a aba "landing" continua sendo a mesma conversa do Claude depois de ir ao FLUXO, e depois de fechar e abrir o programa.
- **Obtido:** `landing@uycf4l → landing@p34vto`.
- **Por quê:** o id que vai para `ponte.enviar` é `id + '@' + CARGA`, e `CARGA` é aleatório a cada carga da página. Daí:
  - `conversa_info` da chave nova é `null`;
  - `preparar` cria um UUID novo;
  - a próxima mensagem começa uma sessão do zero, sem o contexto;
  - eventos de uma resposta em andamento chegam com a carga velha e são jogados fora (`idDaChave` → `null`). A resposta some e o PARAR daquela conversa fica sem botão;
  - o `conversas.json` cresce uma entrada por carga.
- **Arquivo:linha provável:** `ui/ponte-main.js:14` (`const CARGA = Math.random()…`), `:41-42` (`chave`, `idDaChave`).
- **Impacto no checklist:** B12 "retomar" funciona no backend (provado em `conversa.mjs`), mas **não pela tela**.

### A3 · alto · Motor de tokens: 1ª leitura lenta, exe com até 923 MB, leitura quadrática por bloco

- **Reproduzir:**
  - `node ferramentas\testes\qa\memoria-motor.mjs 150`: sessões vivas reais, só leitura;
  - `node ferramentas\testes\qa\tokens.mjs 60`: transcript falso de 60 MB com linhas curtas.
- **Esperado:** a leitura incremental do painel do Rogério: blocos de 4 MB, orçamento por volta, memória baixa.
- **Obtido:**
  ```
  sessões reais: memória privada do exe 128 → … → 923 MB em ~40 s e fica em 923 MB;
  aos 40 s, tokens() ainda tinha 0 sessões (a 1ª volta não tinha terminado)
  falso de 20 MB: 20,8 s · 60 MB: 86,8 s no debug e 77,3 s no RELEASE (~1,3–1,5 s/MB)
  ```
- **Por quê:**
  1. `tokens.rs:122-138` (`linhas_novas`) lê **o arquivo novo inteiro** antes de processar e guarda todas as linhas num `Vec<String>`. O pico de memória acompanha o tamanho do transcript (o maior real tem 358 MB; os de subagente "passam de 1 GB", P00 D9) e o exe não devolve a memória.
  2. `tokens.rs:131-133`: `self.resto.drain(..=i)` linha por linha num `Vec` de 4 MB move o resto do bloco a cada linha. Isso é quadrático por bloco. Com linhas curtas fica em ~1,4 s/MB. Nas linhas reais (~20 KB em média) cai umas 100 vezes. O release ser quase tão lento quanto o debug confirma que o custo é do algoritmo.
  3. `tokens.rs:661-723`: a volta é sequencial. Enquanto um arquivo grande é lido, nada mais anda: a 2ª fonte do "pronto" (`busy` → `idle`) e o CONSUMO ficam parados.
- **Observação:** os transcripts reais somam 4,1 GB em 396 arquivos.

### M1 · médio · Sidecar morto: a Sara fica presa em "Alex, preciso de você."

- **Reproduzir:** `conversa.mjs`, turno 3: pedir um Write; com o cartão aberto, `taskkill /PID <node do sidecar> /F`.
- **Esperado:** a Sara sai de "precisa" e vai para erro ("a conversa fechou") ou para esperando.
- **Obtido:** `estado=precisa · legenda=Preciso de você · aviso=Alex, preciso de você.`, 6 s depois e sem fim. O cartão some e a conversa recebe o erro, mas a miniatura e a placa continuam pedindo o Alex.
- **Arquivo:linha provável:** `lib.rs:713-722` (`sidecar_caiu` limpa as conversas e os cartões, mas não chama `receber_evento`).

### M2 · médio · Duas abas na Main: quando uma termina, a outra perde o "trabalhando" e o PARAR

- **Reproduzir:** `node ferramentas\testes\qa\abas.mjs` (navegador comum, ponte de mentira).
- **Esperado:** a aba A terminar não mexe na aba B, e o PARAR da B continua chegando ao programa.
- **Obtido:**
  ```
  rodandoEm: molduras → null
  PARAR na aba B: parar chamado 0 vez(es)
  ```
  O mesmo vale para o `ponte-main.js` do commit `2298fb6`.
- **Arquivo:linha provável:** `ui/ponte-main.js:64`, `:113`, `:120`, `:126` (um `rodandoEm` só para todas as abas) e `:301` (o PARAR lê esse valor).

### M3 · médio · O "pronto" de uma conversa é apagado pela ferramenta de outra

- **Reproduzir:** `node ferramentas\testes\qa\duas-sessoes.mjs`: hook `pronto` da sessão A e, logo depois, `PreToolUse` (Read) da sessão B.
- **Esperado:** "Alex, seu trabalho está pronto." continua na miniatura até ele ver ou dispensar.
- **Obtido:** `antes: Alex, seu trabalho está pronto. · depois do Read da B: aviso sumiu`. Com duas conversas no terminal ou no VS Code, ele perde o aviso.
- **Arquivo:linha provável:** `estado.rs:145` (`atual.aviso = None` em todo "trabalhando") e o estado único em `lib.rs:38`.

### M4 · médio · Um campo de tipo errado no `config.json` zera a config inteira

- **Reproduzir:** `arquivos-config.mjs`, roteiro 1: `config.json` com `pastaDeTrabalho` certo, `tema: "grade"` e `mudo: "sim"`.
- **Esperado:** vale o que dá para ler. O campo ruim volta ao padrão, e o arquivo que não foi lido não é sobrescrito sem cópia.
- **Obtido:**
  - leitura: `pasta=A:\Claude · tema=metal`, porque tudo voltou ao padrão;
  - depois de um clique em "silenciar": `pastaDeTrabalho=null`, gravado por cima, sem cópia.
  - A pasta de trabalho do Alex, o tema e o canto da miniatura se perdem. Vai pesar quando a config nova (`preferencias.rs`, 18 chaves) mudar algum tipo entre versões.
- **Arquivo:linha provável:** `config.rs:62-65` (`serde_json::from_str(...).ok().unwrap_or_default()` com `#[serde(default)]`, que só cobre campo **ausente**) e `lib.rs:66-72` (`salvar_config` sem cópia).

### M5 · médio · Porta ocupada: nenhum aviso, `porta` velho e token para o outro programa

- **Reproduzir:** `servidor.mjs`, parte "porta ocupada": um processo escuta na 47790 e o arquivo `porta` da abertura anterior diz 47790.
- **Esperado:** o programa tenta outra porta (ou apaga o arquivo `porta` velho) e avisa que os hooks não chegam. O hook nunca manda o token para outro processo.
- **Obtido:**
  ```
  registro: servidor dos hooks NÃO abriu na porta 47790 (os error 10048)
  info.porta=null · estado=esperando/Por aqui (nenhum aviso)
  arquivo porta=47790 (o velho ficou)
  o intruso recebeu X-Ponte-Token com o token verdadeiro
  ```
- **Arquivo:linha provável:** `lib.rs:985-992` (só registra o erro), `config.rs:89-96`, `ferramentas/hook/ponte-hook.mjs:36-44`.

### M6 · médio (segurança) · `conversas_antigas` aceita qualquer pasta

- **Reproduzir:** `arquivos-config.mjs`, roteiro 4: com a pasta de trabalho em `%TEMP%`, chamar `invoke('conversas_antigas', { pasta: 'A:\\Claude', limite: 3 })`.
- **Esperado:** recusa, porque fica fora da pasta de trabalho.
- **Obtido:** `devolveu 3 conversa(s) de A:\Claude (com nome = último pedido)`. O "nome" é o último pedido do Alex naquela conversa.
- **Arquivo:linha provável:** `lib.rs:844-847` (`pasta.map(PathBuf::from)`, sem `arquivos::resolver`).

### M7 · médio (segurança) · A própria página muda a barreira de caminho

- **Reproduzir:** `arquivos-config.mjs`, roteiro 4.
- **Esperado:** a pasta de trabalho só muda pelo seletor do Windows (`trocarPasta`), ou ao menos não aceita pasta do sistema nem caminho relativo.
- **Obtido:**
  - `config.gravar({ pastaTrabalho: 'C:\\Windows' })` → `C:\Windows`;
  - `config.gravar({ pastaTrabalho: '.' })` → `A:\Claude\03-projetos\ponte-de-comando-sara` (a pasta do processo).
  - Toda a barreira do `arquivos.rs` vale só até a página pedir outra raiz.
- **Arquivo:linha provável:** `lib.rs:519-530` (`canonicalize(t.trim())` aceita relativo e qualquer pasta).

### M8 · médio (segurança) · Sem CSP, com fontes da internet e `withGlobalTauri`

- **Prova:**
  - `src-tauri/tauri.conf.json`: `"csp": null`, `"withGlobalTauri": true`;
  - `capabilities/default.json`: "os comandos próprios valem para todas";
  - `ui/mini.html:6-7` e as páginas do `estudio\` carregam `fonts.googleapis.com` (16 ocorrências).
- **Risco:** um único script injetado (dado do Claude parar num `innerHTML` do protótipo, ou um recurso de fora) chama `enviar`, `responder_permissao(…, true)`, `config_gravar` e o resto. Hoje não achei injeção: `ponte-main.js` e `mini.html` usam `textContent`, e o cartão monta um `innerHTML` fixo. As fontes também fazem rede a cada abertura.
- **Já está no P00** (Fase 5, "CSP estrita"). Fica registrado porque hoje não há nenhuma.

### B1 · baixo · Pasta de trabalho gravada que sumiu cai calada no `A:\Claude`

- **Reproduzir:** `arquivos-config.mjs`, roteiro 3.
- **Obtido:** `caiu em A:\Claude (padrão=true) sem aviso na tela`. Uma conversa nova roda então no `A:\Claude` sem ele perceber.
- **Arquivo:** `config.rs:109-117`.

### B2 · baixo · Pasta sem permissão diz "não existe", com o prefixo `\\?\`

- **Reproduzir:** `arquivos-config.mjs`, pasta `trancada` com `icacls /deny RX`.
- **Obtido:** `Essa pasta não existe: \\?\C:\Users\alexs\AppData\Local\Temp\ponte-qa-barreiras\trabalho\trancada`
- **Arquivo:** `arquivos.rs:146` (o erro do `canonicalize` vira sempre "não existe", e usa `display()` no lugar de `exibir()`).

### B3 · baixo · Nomes reservados aceitos

- **Reproduzir:** `arquivos-config.mjs`, roteiro 4.
- **Obtido:** `COM¹.txt`, `CONOUT$` e `NUL .txt` são criados. O `docs\ponte.md` promete recusar `CON/PRN/AUX/NUL/COM1-9/LPT1-9` com ou sem extensão. No Windows 11 o `del` normal ainda apaga, mas programa antigo e Windows 10 tratam esses nomes como dispositivo.
- **Arquivo:** `arquivos.rs:87-93` (não cobre `COM¹²³`/`LPT¹²³`, `CONIN$`/`CONOUT$` nem espaço antes do ponto).

### B4 · baixo · 16 conexões lentas calam o servidor dos hooks

- **Reproduzir:** `servidor.mjs`: 16 conexões mandam 1 byte a cada 2 s.
- **Obtido:** o hook legítimo leva `ECONNRESET`. O limite de 3 s é por leitura, e não para o pedido inteiro, então uma conexão pode durar horas (8 KB × 3 s).
- **Arquivo:** `servidor.rs:27-30`, `:61`, `:76-97`. Só um processo local consegue fazer isso.

### B5 · baixo · Resposta 413 perdida por RST (o teste do cargo que falha às vezes)

- **Reproduzir:** `servidor.mjs`: corpo de 200 KB. Na 1ª rodada o cliente leu o 413; na 2ª levou `ECONNRESET` antes de ler.
- **Por quê:** o servidor responde e fecha sem ler o corpo que ainda está chegando, e o Windows manda RST, que pode apagar a resposta. É o mesmo motivo de `servidor::testes::token_e_origem` falhar em 2 de 16 rodadas do `npm run testar` (`ConnectionReset 10054` em `servidor.rs:135`).
- **Arquivo:** `servidor.rs:102-110` (403, 401 e 413 saem sem drenar o corpo nem fazer `shutdown(Write)` antes de fechar).

### B6 · baixo (segurança) · Comandos expostos que nenhuma tela usa

- **Prova:** busca em `ui\ponte.js`, `mini.html`, `carregamento.html`, `ponte-main.js` e `estudio\*`.
- **Nenhuma tela chama:** `mini_soltou` (nem a `ponte.js` expõe; `arquivos-config.mjs` mostra que ele responde), `parar_agente`, `conversas_antigas`, `retomar_sessao`, `conta`, `conexoes_mcp` (abre os servidores MCP do Alex), `limites`, `config_gravar` (só os testes) e `info_app` (só os testes).
- Fazem parte da Fase 2 (P00), mas hoje são superfície sem uso. O mais sensível é o M6.
- **Arquivo:** `lib.rs:1072-1104`.

### B7 · baixo (segurança) · O release obedece variáveis de ambiente de teste

- **Prova:** `tokens.mjs` rodou no `target\release` (`PONTE_QA_EXE`), e o Playwright entrou pela porta de depuração.
- **Risco:** quem puser `PONTE_DEPURAR_PORTA` no ambiente do Alex controla a Ponte instalada pelo CDP, e isso inclui clicar PERMITIR.
- **Mesma família:** `PONTE_SIDECAR` (script que o node roda), `PONTE_NODE`, `PONTE_CLAUDE` + `PONTE_CLAUDE_PREFIXO`.
- **Arquivo:** `lib.rs:181-185`, `lib.rs:934-936`, `sidecar.rs:78-83`, `conversa.rs:282-285`.

### B8 · baixo (segurança) · Processos filhos

- **O que está certo:**
  - todos os `Command` do Rust têm `CREATE_NO_WINDOW` (`arquivos.rs:361`, `conversa.rs:478`, `sidecar.rs:127`, `tokens.rs:826`);
  - o SDK usa `windowsHide: true`;
  - todos os `spawn` dos testes têm `windowsHide`.
- **O que falta:**
  - `claude` **pelo nome** quando o `.local\bin\claude.exe` não existe: `conversa.rs:292`, `tokens.rs:813`;
  - `achar_node` aceita entrada relativa do PATH (`sidecar.rs:84-90`). Hoje o PATH não tem nenhuma;
  - `rodar_claude` não lê o stderr (`tokens.rs:819`): se o `claude mcp list` escrever muito ali, trava até o tempo limite. O `kill` do tempo limite não mata os netos (servidores MCP);
  - nem ele nem o `claude -p` do plano B (`conversa.rs:480`) entram no Job Object.

### B9 · baixo (teste) · `limpar.mjs` nunca mata nada

- **Prova:**
  ```
  node -e "const PS='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'; …"
  → "C:WindowsSystem32WindowsPowerShell\u000b1.0powershell.exe" "C:WindowsSystem32\taskkill.exe" · erro: ENOENT
  ```
- O `spawnSync` falha calado e devolve lista vazia. Se o exe cair no meio de um teste, os WebView2, o node e o claude ficam.
- **Arquivo:** `ferramentas/testes/limpar.mjs:5-6` (`\W` vira `W`, `\v` vira tab vertical, `\t` vira tab). O `qa/comum.mjs` usa a versão certa.

### B10 · baixo (teste) · `testar:programa` com teste que não testa

- `programa.mjs:110-113`, "silenciar chega na miniatura e é gravado": liga e desliga o mudo, e só confere que o arquivo **não** tem `"mudo": true` no fim. Passaria mesmo se nunca gravasse nada.
- `programa.mjs:173`: só faz `taskkill /T` se o exe ainda estiver vivo. Se ele cair, sobra tudo, e o `limparProcessos` (B9) não salva.
- O `miniatura.mjs` (12 testes) não está em nenhum script do `package.json`.

### B11 · baixo (teste) · A instância única é a do programa instalado

- **Prova:** `ciclo.mjs`: a segunda abertura, com outra pasta de dados, foi absorvida pela primeira. O plugin single-instance usa o `identifier`, e não a pasta de dados.
- **Risco:** com a Ponte instalada aberta, `npm run testar:programa` não abre o exe de teste. Ele chama `mostrar_principal` na Ponte **do Alex** (foco na janela dele), e o teste falha no CDP.
- **Arquivo:** `lib.rs:948-950`.

---

## 3. NÃO FOI POSSÍVEL VALIDAR

- **A miniatura de verdade fora da tela depois de trocar de monitor ou de resolução.** No modo de teste, `posicao_esperada` sempre devolve −28000 (`lib.rs:309-311`). Mostrar a miniatura de verdade punha uma janela por cima na tela do Alex. Validei só a parte de dados: monitor inexistente e canto inválido caem no canto padrão do monitor principal. O cálculo da área útil, o `ScaleFactorChanged` e o arrasto com o mouse de verdade ficaram sem teste.
- **CPU com as janelas na posição real.** Medi com a principal visível a −30000. O caso "escondido" (C1) não depende disso.
- **O trabalho não commitado de 02:08 em diante** (conta, Cofre, preferências, autostart): não testado.
- **Hook instalado no `settings.json`:** não instalado, por regra. Os hooks foram simulados com os exemplos e com POST direto.

---

## 4. Checklist do Rogério: o que está pronto de verdade

**Legenda:**
- **pronto** = funciona e um teste prova;
- **parcial** = funciona com buraco provado;
- **só dados** = o backend entrega, mas nenhuma tela usa;
- **falta** = não feito.

| Itens | Situação | Por quê |
|---|---|---|
| A1–A7 conversas, custo, contexto, subagentes, skills, por modelo, desconhecido | só dados (A1–A3 e A6 parecem no CONSUMO da Main) | `testar:programa` mediu 5 sessões; A4, A5 e A7 só no `tokens()` |
| A8 leitura incremental e barata | **parcial** | A3: 1ª volta de 40 s a 2 min, 923 MB |
| A9 sessão some quando o processo morre / após 5 min parada | **parcial** | PID vivo sim; os 5 min e o PID reaproveitado (`procStart`) não |
| A10, A12 foco do VS Code | falta (decidido fora) | — |
| A11, A13, A14, A16 miniatura sem borda, por cima, bandeja, cresce com o conteúdo | pronto | `miniatura.mjs` 12/12 |
| A15 lembra a posição e volta para dentro da tela | **parcial** | dados provados; posição real NÃO VALIDADA |
| A17 instância única; `--fechar` de fora | **parcial** | instância única pronta (`ciclo.mjs`); `--fechar` falta |
| A18 repouso, troca de monitor, GPU caída | **parcial** | só o vigia de 3 s |
| A19, F7 registro com teto, sem pasta pessoal | pronto | testes do cargo |
| A20 dica por extenso | pronto (no CONSUMO) | — |
| A21 modo foto, A22 conferência independente | falta | — |
| B1, B3 resposta ao vivo e "terminou" | pronto | `testar:programa`, `conversa.mjs` |
| B2 estados honestos | **parcial** | A1 (PARAR = erro), M1 (preso em "precisa") |
| B4–B6 cartão em PT, tudo pela porta, regras da pasta | pronto | cartão negado (casa) e aprovado (`conversa.mjs`) |
| B7 modos acceptEdits/plan | [decidir] | — |
| B8 parar sem perder | **parcial** | a conversa continua (provado), mas avisa erro (A1) |
| B9 trocar modelo e esforço no meio | pronto no backend, **sem teste meu** | — |
| B10 conta | só dados | `conta()` respondeu |
| B11 comandos `/` | falta | — |
| B12 retomar; bifurcar | **parcial** | retomar pelo backend provado; pela tela não (A2); bifurcar falta |
| B13 diff, B14 terminal à vista, B17 ações no trecho | falta | — |
| B15, B16 parar a árvore e encerrar tudo ao fechar | pronto no SDK | `testar:job`; o plano B `claude -p` fica fora do Job (B8) |
| C1–C3 agentes rodando, mapa, meta.json | só dados (o Fluxo não usa) | — |
| C4 parar um agente | só dados | `parar_agente`, sem tela e sem teste |
| C5 resumo de progresso desligado | pronto | `ponte-sidecar.mjs:120` |
| D1 relógio do cache | só dados | — |
| D2 conversa pelo id | pronto | — |
| D3 só as conversas desta janela | falta | — |
| D4, D5 linha que encolhe, três estados honestos | falta | — |
| D6 skill pela barra | pronto (backend) | — |
| E1, E2 barras 5 h / 7 d, perder nunca | só dados | `limites()` provado com e sem `cachedUsageUtilization` |
| E3 tela do consumo | falta | — |
| E4, E5 conta e MCP | só dados (entrar e sair, falta) | — |
| F1, F2 conversas de outros dias e nome | só dados | e sem barreira de pasta (M6) |
| F3 renomear, F5 skills clicáveis, F6 socorro | falta | — |
| F4 nunca sem pasta | pronto | com a ressalva do B1 |

**Resumo dos 62 itens:**
- pronto: 19;
- parcial: 8;
- só dados (backend sem tela): 19;
- falta ou decidido fora: 15;
- [decidir]: 1.

---

## 5. Passagem

- **`eng-desktop` (Davi):** C1, A1, A2, A3, M1–M5, B1–B5, B9–B11. Cada um tem o teste que falha em `ferramentas\testes\qa\`. Reconferir na build nova, porque o código mudou durante o teste.
- **`eng-seguranca` (Heitor):** M6, M7, M8, B6, B7, B8, e a superfície nova de conta e chave de API (`cofre.rs`, `conta_*`), que não foi auditada.
- **Sara:** a Main da fonte (`estudio\Main.dc.html`) precisa pausar os vídeos e as animações quando a página ficar escondida (C1). A2 e M2 somem quando o template tiver a conversa dele com id estável por aba.

---

## 6. Status das correções (Davi, `eng-desktop`, 06/11/2026, ramo `pausa/2026-11-06-correcoes-qa`)

Medido no `target\debug` novo, isolado e fora da tela, como no relatório.

| # | Status | Como | Teste |
|---|---|---|---|
| C1 | **corrigido** (resíduo) | A casca avisa o WebView2 (`SetIsVisible`) e a página (`document.hidden` = true) quando a janela some ou minimiza; vídeos e animações pausam (`PAUSA_JS` em `lib.rs`); a miniatura escondida também esconde a raiz (janela transparente seguia desenhando). A Main escondida não relê o consumo. | `visibilidade.mjs` **2/2** (0 quadros/s nas duas). `cpu.mjs --claude-vazio`: na tela ~270 %, tudo escondido **3,4 %** em regime → ainda acima do limite de 2 % do teste: **FALHOU por pouco**. O piso medido com as duas páginas em branco e o CDP ligado foi 1,4–2,7 %. Nos primeiros ~10 s depois de esconder o WebView2 ainda solta a GPU (o `cpu.mjs` passou a medir 20 s depois). Só a miniatura: 313 % → 32 %. |
| A1 | **corrigido** | O sidecar marca o turno parado; o `result` interrompido vira `resultado` com `cancelado: true, erro: false`; a casca e a Main não tratam como erro. | `conversa.mjs` **16/16** |
| A2 | **corrigido** | Id estável `aba@main`; `conversa()` traz `rodando` e a Main volta a mostrar o PARAR depois de recarregar. | `ciclo.mjs`: "ir ao FLUXO e voltar mantém o id" OK |
| A3 | **corrigido** | `tokens.rs`: `BufReader` + `read_until`, linha direto para a conta, orçamento de 64 MB por volta, sessão em 1ª leitura aparece como "lendo". | `tokens.mjs 60` **4/4** (60 MB em 1,0 s, pico 9 MB; antes 86,8 s); `memoria-motor.mjs` **1/1** (pico 9 MB, antes 923 MB); cargo `leitura_incremental_por_orcamento` |
| M1 | **corrigido** | `sidecar_caiu` leva a Sara para "erro" quando havia conversa rodando ou cartão aberto. | `conversa.mjs` (sidecar morto) OK |
| M2 | **corrigido** | Conjunto de abas rodando no adaptador; `rodandoEm` aponta para a aba aberta; PARAR usa a aba aberta. | `abas.mjs` **3/3** (o teste passou a usar a aba `estudio`: `molduras` não existe mais no template). Com o adaptador antigo: 1/3. |
| M3 | **corrigido** | O aviso guarda a sessão; "trabalhando"/"esperando" de outra sessão não apaga. Conversas do programa mandam a sessão delas. | cargo `outra_sessao_nao_apaga_o_pronto`; `duas-sessoes.mjs` não rodado nesta rodada |
| M4 | corrigido (pausa, 37a445b) | Leitura campo a campo + cópia `config.corrompido-*.bak`. | `arquivos-config.mjs` roteiro 1 — não rodado nesta rodada |
| M5 | corrigido | Tenta as 10 portas seguintes, `porta` com id da execução, `/ping` antes do token; aviso na faixa da Main. | `hook.test.mjs` (intruso não recebe o token); `servidor.mjs` não rodado nesta rodada |
| M6 | corrigido (pausa) | `conversas_antigas` passa pelo `arquivos::resolver`. | `arquivos-config.mjs` (aceita a recusa como promessa rejeitada) — não rodado nesta rodada |
| M7 | corrigido (pausa) | `pastaTrabalho` só aceita `null`; trocar é só pelo seletor (que recusa raiz de disco e `C:\Windows`). | idem |
| M8 | **corrigido** (parcial) | CSP estrita em `tauri.conf.json`; cada janela com só os comandos que usa. Fica: `withGlobalTauri` (a `ponte.js` precisa) e as fontes do Google (estão nos HTML da `ui\`, que é da Lia). | `programa.mjs --sem-claude` **27/27** e `paginas` 14/14 com a CSP |
| B1 | corrigido | Faixa "A pasta de trabalho gravada não existe mais…" (conferida a cada estado). | `arquivos-config.mjs` roteiro 3 — não rodado nesta rodada |
| B2, B3 | corrigidos (pausa) | Mensagem "sem permissão" com `exibir()`; `COM¹`, `CONIN$/CONOUT$`, `NUL .txt`. | cargo `nomes` |
| B4 | corrigido (pausa) | Prazo TOTAL de 3 s por conexão, até 64 conexões. | cargo `conexao_lenta_nao_segura_o_servidor` |
| B5 | corrigido | Lê o corpo antes de responder e fecha com calma (lê o resto por até 250 ms). | cargo `token_origem_ping_e_porta_ocupada` (40 rodadas não feitas) |
| B6 | corrigido | Capabilities por janela (`principal`, `mini`, `carregamento`); `mini_soltou` removido. | `programa.mjs` 27/27 |
| B7 | corrigido | `env_teste` (só debug) para PONTE_*; release tira PONTE_* do sidecar. | revisão de código |
| B8 | corrigido | `claude.exe` só por caminho absoluto; `node` só de entrada absoluta; stderr do `rodar_claude` descartado; filhos no Job; tempo esgotado derruba a árvore. | revisão de código |
| B9 | corrigido | Barras dobradas; erro do spawn agora aparece. | uso pelo `programa.mjs` |
| B10 | corrigido | "silenciar" confere ligado e desligado no arquivo; o fim sempre derruba a árvore. | `programa.mjs` 27/27 |
| B11 | corrigido | No debug com `PONTE_PASTA_DE_DADOS`, o `identifier` ganha um sufixo pela pasta. | `ciclo.mjs` "abrir duas vezes" OK |

**Ainda falha em `ciclo.mjs`:** "tudo escondido < 1 %" (15,9 % medido só 3 s depois de esconder; ver C1) e "30 trocas + 400 eventos: memória < +60 MB" (+174 MB). O `vazamento.mjs` (o teste próprio de vazamento) não foi rodado nesta rodada.

**Suítes:** `npm run testar` node 9/9 + cargo 40 ok (1 ignorado) · `testar:paginas` 14/14 · `programa.mjs --sem-claude` 27/27 · QA: `visibilidade` 2/2, `tokens` 4/4, `memoria-motor` 1/1, `conversa` 16/16, `abas` 3/3, `ciclo` 5/8 (a 3ª falha era o próprio teste chamando `ponte.conta()`, já ajustado para `ponte.conta.status()`, sem rodar de novo), `cpu` 0/1 (3,4 %).
