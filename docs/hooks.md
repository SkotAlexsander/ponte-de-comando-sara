# Hooks do Claude Code → Ponte de Comando

Os hooks fazem as conversas abertas **fora** do programa (no terminal ou no VS Code) avisarem a miniatura: "trabalhando", "preciso de você" e "pronto".

> **Nada disto foi instalado.** Colar o bloco no `settings.json` muda a configuração do Claude Code do Alex, então só depois do "sim" dele.
> Sem os hooks, o programa ainda percebe o "pronto" por uma segunda fonte, que é `~/.claude/sessions/<pid>.json` passando de `busy` para `idle`. Os hooks deixam o aviso mais rápido e trazem "trabalhando com X" e "preciso de você".

## O que cada hook vira

| Hook | Evento na Ponte | Na miniatura |
|---|---|---|
| `UserPromptSubmit` | `trabalhando` / `pedido`, com o título (140 letras do pedido) | "SARA · TRABALHANDO · <pedido>" |
| `PreToolUse` | `trabalhando` / `ferramenta` | "Lendo LEIA-ME.md", "Rodando: npm test"… |
| `PostToolUse` | `trabalhando` / `ferramenta-fim` | idem |
| `SubagentStop` | `trabalhando` / `subagente` | "Um funcionário terminou a parte dele" |
| `Notification` | `precisa`, com a mensagem | aviso "Alex, preciso de você.", som e notificação do Windows |
| `Stop` | `pronto`, com o pedido e o resumo lidos do fim do transcript | aviso "Alex, seu trabalho está pronto.", som e notificação do Windows |

O hook:
- **nunca escreve no stdout**, porque no `UserPromptSubmit` o stdout entraria no contexto do Claude;
- sai **sempre com código 0**, em até ~2,5 s, mesmo com o programa fechado;
- manda só o necessário: o título do pedido, o nome da ferramenta, a linha curta e o nome da pasta. Não manda conteúdo de arquivo nem comando inteiro.

O "Claude is waiting for your input", que vem depois de um pronto, não apaga o aviso de pronto. Um pronto repetido em menos de 10 s (hook + sessão viva + conversa do programa) não avisa duas vezes.

## O bloco para colar

Vai em `C:\Users\alexs\.claude\settings.json`, dentro de `"hooks"`. **Se já existir uma chave `"hooks"`, junte as listas**: não troque a chave inteira, senão os hooks que já estão lá somem.

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [ { "type": "command", "command": "node \"A:\\Claude\\03-projetos\\ponte-de-comando-sara\\ferramentas\\hook\\ponte-hook.mjs\"", "timeout": 5 } ] }
    ],
    "PreToolUse": [
      { "matcher": "*", "hooks": [ { "type": "command", "command": "node \"A:\\Claude\\03-projetos\\ponte-de-comando-sara\\ferramentas\\hook\\ponte-hook.mjs\"", "timeout": 5 } ] }
    ],
    "PostToolUse": [
      { "matcher": "*", "hooks": [ { "type": "command", "command": "node \"A:\\Claude\\03-projetos\\ponte-de-comando-sara\\ferramentas\\hook\\ponte-hook.mjs\"", "timeout": 5 } ] }
    ],
    "SubagentStop": [
      { "hooks": [ { "type": "command", "command": "node \"A:\\Claude\\03-projetos\\ponte-de-comando-sara\\ferramentas\\hook\\ponte-hook.mjs\"", "timeout": 5 } ] }
    ],
    "Notification": [
      { "hooks": [ { "type": "command", "command": "node \"A:\\Claude\\03-projetos\\ponte-de-comando-sara\\ferramentas\\hook\\ponte-hook.mjs\"", "timeout": 5 } ] }
    ],
    "Stop": [
      { "hooks": [ { "type": "command", "command": "node \"A:\\Claude\\03-projetos\\ponte-de-comando-sara\\ferramentas\\hook\\ponte-hook.mjs\"", "timeout": 5 } ] }
    ]
  }
}
```

**Custo:** cada hook abre um `node` (uns 50 a 100 ms). `PreToolUse` e `PostToolUse` rodam em toda ferramenta. Se pesar, tire o `PostToolUse` (o `PreToolUse` já dá a linha) ou os dois, e fique só com `UserPromptSubmit`, `Notification` e `Stop`.

**Conversas abertas pelo próprio programa** também carregam os settings do usuário, então os hooks rodam nelas. Não tem problema: os eventos se repetem e a casca não avisa duas vezes.

## Onde ficam o token e a porta

- Token: `%APPDATA%\com.pixelmartins.pontedecomando\token` (64 letras hexadecimais, criado na primeira abertura).
- Porta em uso: `%APPDATA%\com.pixelmartins.pontedecomando\porta`. A padrão é **47615**, e dá para trocar em `config.json` → `"porta"`.
- O servidor só escuta em `127.0.0.1`, exige o cabeçalho `X-Ponte-Token`, recusa pedido de navegador (com `Origin`) e corpo acima de 64 KB.

## Testar

Com o programa aberto:

```
node ferramentas\hook\ponte-hook.mjs < ferramentas\hook\exemplo-stop.json
```

A miniatura mostra "Alex, seu trabalho está pronto." com "curso-mirian: Sara, faz a landing do curso da Mirian…", toca o som e sai a notificação do Windows.

Para ver o que aconteceu, use `--verboso`. Ele escreve no **stderr**, por exemplo `ponte-hook: enviado (pronto) → HTTP 200`:

```
node ferramentas\hook\ponte-hook.mjs --verboso < ferramentas\hook\exemplo-stop.json
node ferramentas\hook\ponte-hook.mjs --verboso < ferramentas\hook\exemplo-prompt.json
node ferramentas\hook\ponte-hook.mjs --verboso < ferramentas\hook\exemplo-pretool.json
node ferramentas\hook\ponte-hook.mjs --verboso < ferramentas\hook\exemplo-notification.json
```

Os exemplos são regravados por `node ferramentas\hook\gerar-exemplos.mjs`. Os testes automáticos estão em `ferramentas\testes\hook.test.mjs` (`npm run testar`).
