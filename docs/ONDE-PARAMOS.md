# Pausa: 06/11/2026, ~20:00. Retomar AQUI

**Em andamento quando pausou: o modo "Controle total com guarda"** (pedido do Alex: "controle total das ações, respeitando apenas passos de segurança para cuidar se algo pode prejudicar o sistema ou identificar arquivos de coletas").
- **Davi:** `sidecar/guarda.mjs` + `ferramentas/testes/guarda.test.mjs` + `guarda-programa.mjs` (WIP, neste commit; NÃO testado). Ele estava escrevendo a bateria de testes.
  - Especificação: `aprovacao: 'guarda' | 'sempre'` (padrão `'guarda'`), sempre pelo `canUseTool`, nunca `bypassPermissions`.
  - O cartão aparece só nos 8 grupos de risco: apagar em massa ou fora da pasta; sistema; segurança/Defender; segredos e coleta; mandar dados para fora; persistência; coisa baixada ou ofuscada; áreas protegidas (Valores, livro, diário, settings.json).
  - Na dúvida, falha fechada. Registro em `%APPDATA%\com.pixelmartins.pontedecomandocoes.log`, exposto como `ponte.registro.ler(n)`.
  - O script do print (user32 PrintWindow/SetCursorPos/mouse_event) tem que passar direto.
- **Lia:** a escolha CONTROLE TOTAL COM GUARDA / PEDIR SEMPRE em Novas conversas, o "VER O REGISTRO" e o "restaurar padrão" (branco + atual, lembrar desligado, guarda), quase pronto em `scratchpad\proc\Main.modelo.html`, com cópia no repo empresa (backup `v16-antes-aprovacao`). Faltava conferir a Aparência e rodar os testes.
- **Depois:** o Davi termina os testes → a Lia fecha → `sincronizar-ui` → 0.1.2 → **o Heitor audita a guarda ANTES de instalar** → instalar por cima (como administrador, `/S`).

---

# Onde paramos: 06/11/2026, ~19:45

## Atualização da noite (06/11)
- **0.1.1 INSTALADA no PC do Alex** (NSIS currentUser, rodado como administrador a pedido dele) em `%LOCALAPPDATA%\Ponte de Comando`, com atalho na Área de Trabalho e no Menu Iniciar. Abre em ~10 s; o hook chega (HTTP 200).
  - O Alex ainda não decidiu sobre abrir SEMPRE como administrador; a recomendação é não.
- **Ramo `pausa/2026-11-06-correcoes-qa`** (até `8154eff`, com push; SEM merge no master):
  - correções de QA: C1 (3,4 % de CPU escondida, o limite do teste é 2 %), A3 (tokens incremental, 9 MB), A1, A2, M1–M3, M5, M8 parcial, B5 e B7–B11;
  - a interface nova da Lia (temas + Configuração em 7 seções PT/EN + texto longo/textarea) foi sincronizada e ligada ao backend; a tradução das chaves fica em `ui/ponte-main.js`.
- **Canvas versão 49** com os temas e a Configuração. As fontes da interface estão no repo `empresa` (commit 3a31fd3).
- **Hooks do settings.json consertados:** o caminho estava com caracteres de controle e nunca funcionou antes de 06/11 à noite.
- **Não usar o modelo Fable** (pedido do Alex).

## Próximos passos
1. O Heitor audita: `cofre.rs`, os comandos `conta_*` em `lib.rs` (o `conta_testar_chave` recebe a chave pela IPC), a camada de conta em `ui/ponte-main.js`, `sidecar/regras.mjs`, `capabilities/*.json`, `build.rs` e a CSP. O `conta_trocar` abre um terminal visível (`claude auth login`): ele decide se pode.
2. O Otávio roda de novo `ciclo`, `cpu`, `servidor`, `arquivos-config`, `duas-sessoes` e `vazamento`.
3. A Lia acerta o "restaurar padrão" da tela para os padrões do backend: fósforo branco + letra atual e `lembrar` desligado.
4. Merge no master → instalador 0.1.2 → convite ao Rogério (rogeriofleming). O Paulo ainda vai passar o usuário do GitHub.
5. Telas para os dados do backend (mapa de agentes real, limites, relógio do cache, conversas antigas) e do checklist do Rogério (diff, terminal visível, comandos `/`).
6. Fase 3: a palavra "Sara".

---

## Registro da pausa anterior (06/11, ~02:30)

O Alex pausou o projeto por hoje. Pelo pedido dele, nada foi instalado no PC (o programa) nem enviado a ninguém.

## O que está pronto
- **Programa (Tauri 2 + sidecar Node com o Claude Agent SDK):** abre de ponta a ponta:
  - carregamento de pelo menos 8 s → principal → miniatura;
  - hook de "pronto";
  - conversa real com permissão em português;
  - árvore de arquivos real, criar pasta e arquivo;
  - tokens e limites.
  - A última versão inteira está no ramo `master`, commit `8887de6` + `2298fb6`.
- **Repositório PRIVADO:** https://github.com/SkotAlexsander/ponte-de-comando-sara
- **Hooks do Claude Code:** INSTALADOS no settings do Alex desde 06/11, 01:38. O backup do settings está em `C:\Users\alexs\.claude\settings.json.antes-ponte-20261106-013835`.
- **Interface (canvas):** https://claude.ai/artifact/36y5g7F4nrXyjVwA5oTFqd, versão 47, com as correções do Otto, os 10 efeitos e a Biblioteca nova.
  - Pranchas: Main, Fluxo (ETAPAS/MAPA), Mini, Carregando, Estilos (A/B/C) e Terminal (tema B v2).

## O que ficou no meio
1. **Ramo `pausa/2026-11-06-correcoes-qa`** (commit `37a445b`): correções do relatório de QA EM ANDAMENTO.
   - No último ponto visto compilava e 36 testes passavam.
   - Está incompleto: a faixa de aviso da pasta que sumiu e da porta ocupada estava sendo feita.
   - Faltam conferir: C1 (CPU com janela escondida), A1 (PARAR ≠ erro), A2 (id da conversa estável), A3 (motor de tokens incremental), M1–M8 e B1–B11.
   - Lista completa: `docs/qa/relatorio-qa-01.md`. Testes que falham: `ferramentas/testes/qa/`.
   - Depois: o Heitor (eng-seguranca) audita M6–M8, B6–B8 e o código novo de conta e cofre (`cofre.rs`).
2. **Configurações + temas** (a Lia, design-ui, parou no meio). As fontes estão em `A:\Claude\00-empresa\corpos\sara\interface\estudio\ferramentas\` (`Main.modelo.html` + os scripts `remendo-*`, `temas-paineis*`, `teste-config.py`, `teste-temas*`, `teste-texto-longo.py`, `teste-setas.py`). Ainda NÃO estão publicados no canvas.
   - Temas de display: Metal (padrão), Terminal de fósforo (âmbar, verde ou branco; letra terminal ou atual) e Grade de luz.
     - Valem para TODOS os painéis, mas as molduras de metal ficam.
     - **Aprovado pelo Alex: fósforo BRANCO + letra ATUAL** (régua de qualidade).
   - Texto longo quebrando a linha nos balões + campo `#msg` como textarea que cresce (Enter envia, Shift+Enter quebra a linha).
   - Títulos de painel sem borda branca e sem fundo atrás. Setas a ≥ 10 px da moldura.
   - Tirar a legenda de baixo do retrato da Main (a placa basta; deixar só um sr-only).
   - Pausar vídeos e animações com `document.hidden` (QA C1, parte da tela).
   - Configuração "inteligente" em 7 seções: Aparência, Idioma (PT-BR/EN com i18n de verdade; idioma das respostas), Conta e conexão (trocar conta, chave de API no Cofre do Windows), Novas conversas (padrões; aprovação sempre ligada), Avisos, Sistema e Sobre.
3. **Lado de trás da Configuração** (eng-desktop): `ponte.config` com as chaves novas, `ponte.conta.*` (status, trocar, sair, chave de API no Credential Manager), `ponte.sistema.*`. Parte disso já estava começada no ramo de pausa.

## Para retomar (ordem sugerida)
1. Terminar as correções de QA no ramo de pausa → rodar todas as suítes (da casa e do QA) → merge no `master`.
2. A Lia termina temas e Configuração → a Sara publica no canvas → `npm run sincronizar-ui` no programa.
3. O Heitor audita a segurança. O Otávio refaz o QA.
4. Telas que faltam para os dados que o backend já tem: mapa de agentes real, barras de limite, relógio do cache, conta e conversas antigas. Do checklist do Rogério (19/62 prontos): revisão por diff, terminal visível e comandos `/`.
5. Instalar no PC do Alex (o Smart App Control pode barrar; nunca contornar). Depois mandar convite ao Rogério (rogeriofleming). O Paulo ainda vai passar o usuário do GitHub.
6. Fase 3: ativar a conversa falando "Sara".

## Regras que valem
- A miniatura só aparece com a principal minimizada ou na bandeja.
- A notificação do Windows só sai com nada visível e com o programa instalado.
- O modelo, o esforço e a velocidade são por conversa.
- A pasta de trabalho padrão é `A:\Claude`.
- Nunca usar o modo que age sem aprovação.
