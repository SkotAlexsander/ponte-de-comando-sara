// A GUARDA do modo "Controle total com guarda" (aprovacao: 'guarda').
//
// Continua tudo passando pelo canUseTool (nunca bypassPermissions): a guarda só decide se o pedido
// é liberado sozinho ou se vai para o cartão do Alex, com a categoria e o motivo em PT-BR.
// Regra de ouro: na dúvida, CARTÃO (falha fechada). Ferramenta desconhecida, MCP, script ofuscado ou
// que a guarda não consegue ler sempre vai para o cartão.
//
// Testes: ferramentas/testes/guarda.test.mjs (npm run testar).

export const CATEGORIAS = {
  apagar: 'Apaga em massa ou fora da pasta de trabalho',
  sistema: 'Mexe no sistema',
  seguranca: 'Mexe na segurança do Windows',
  segredos: 'Segredos ou coleta de dados',
  envio: 'Manda dados para fora',
  persistencia: 'Cria persistência',
  baixado: 'Roda coisa baixada ou ofuscada',
  protegido: 'Toca em área protegida do Alex',
  fora: 'Fora da pasta de trabalho',
  desconhecido: 'A guarda não conseguiu analisar',
}

// ferramentas que só leem, conversam ou organizam: liberadas (os caminhos de leitura ainda passam pelas regras)
const SO_LEITURA = new Set(['Read', 'Glob', 'Grep', 'LS', 'NotebookRead'])
const ESCRITA = new Set(['Write', 'Edit', 'MultiEdit', 'NotebookEdit'])
const INOFENSIVAS = new Set(['TodoWrite', 'Task', 'Agent', 'WebFetch', 'WebSearch', 'ExitPlanMode', 'Skill', 'ToolSearch', 'AskUserQuestion', 'BashOutput', 'KillShell', 'TaskOutput', 'TaskStop', 'ListMcpResourcesTool', 'ReadMcpResourceTool'])
const SHELL = new Set(['Bash', 'PowerShell'])

const MAX_COMANDO = 20000

/** Texto para comparar: minúsculo, sem aspas, crases nem ^ (driblar com r"m" ou I`nv`oke não passa). */
function normal(t) {
  return String(t || '').toLowerCase().replace(/["'`^]/g, '')
}

/** Caminho comparável: barras invertidas, /c/... (Git Bash) vira c:\..., sem barra no fim. */
export function caminho(p) {
  let s = String(p || '').trim().replace(/["']/g, '').replace(/\//g, '\\').toLowerCase()
  const gb = /^\\([a-z])\\/.exec(s)
  if (gb) s = gb[1] + ':\\' + s.slice(3)
  if (/^\\cygdrive\\([a-z])\\/.test(s)) s = s.replace(/^\\cygdrive\\([a-z])\\/, '$1:\\')
  return s.replace(/\\+$/, '')
}

function dentro(p, pasta) {
  const a = caminho(p), b = caminho(pasta)
  if (!a || !b) return false
  if (/(^|\\)\.\.(\\|$)/.test(a)) return false
  return a === b || a.startsWith(b + '\\')
}

const absoluto = (p) => /^[a-z]:\\/.test(caminho(p)) || /^\\\\/.test(caminho(p))

// ---------------------------------------------------------------- caminhos sensíveis (ler ou escrever)

const SEGREDO_CAMINHO = [
  [/(^|\\)\.env(\.(local|production|development|prod|dev|test))?$/, 'arquivo .env (segredos)'],
  [/\.pem$/, 'chave .pem'],
  [/(^|\\)id_(rsa|dsa|ecdsa|ed25519)[^\\]*$/, 'chave SSH'],
  [/github-recovery-codes/, 'códigos de recuperação do GitHub'],
  [/com\.pixelmartins\.pontedecomando\\token$/, 'o token da própria Ponte'],
  [/taylor-config-pessoal\\\.env/, 'o .env pessoal do Taylor'],
  [/\\(login data|cookies|local state|web data)( |$|-)/, 'dados do navegador (senhas, cookies)'],
  [/\\(logins\.json|key4\.db|cookies\.sqlite)$/, 'dados do navegador (senhas, cookies)'],
  [/\\microsoft\\(credentials|vault|protect)(\\|$)/, 'Cofre do Windows'],
]
const PROTEGIDO_CAMINHO = [
  [/com\.pixelmartins\.valores/, 'dados do Valores (são do Alex, digitados à mão)'],
  [/livro-sangue-sintetico|sangue-sintetico-leitores-beta/, 'o livro do Alex'],
  [/conversas-com-claude/, 'o diário do Alex'],
  [/\\\.claude\\settings(\.local)?\.json$/, 'o settings.json do Claude Code'],
]
// onde o próprio Claude Code escreve (memória do projeto e rascunho): liberado como a pasta de trabalho
const areasDoClaude = () => [
  process.env.USERPROFILE ? process.env.USERPROFILE + '\\.claude\\projects' : '',
  process.env.TEMP ? process.env.TEMP + '\\claude' : '',
].filter(Boolean)
const SISTEMA_CAMINHO = /^([a-z]:\\windows|[a-z]:\\program files( \(x86\))?|[a-z]:\\programdata)(\\|$)/

function olharCaminho(p, escrever, pasta) {
  const c = caminho(p)
  for (const [re, motivo] of PROTEGIDO_CAMINHO) if (re.test(c)) return perguntar('protegido', `${escrever ? 'Escrever' : 'Ler'} em ${motivo}.`)
  for (const [re, motivo] of SEGREDO_CAMINHO) if (re.test(c)) return perguntar('segredos', `${escrever ? 'Escrever' : 'Ler'} ${motivo}.`)
  if (escrever) {
    if (SISTEMA_CAMINHO.test(c)) return perguntar('sistema', 'Escrever numa pasta do sistema (Windows, Program Files ou ProgramData).')
    if (!dentro(c, pasta) && !areasDoClaude().some((a) => dentro(c, a))) return perguntar('fora', 'Escrever fora da pasta de trabalho.')
  }
  return null
}

// ---------------------------------------------------------------- comandos (Bash e PowerShell)

const REGRAS = [
  // 1. apagar em massa ou de forma irreversível
  ['apagar', /\brm\s+(-[a-z]*r[a-z]*|-[a-z]*\s+-[a-z]*r|--recursive)\b/, 'comando rm recursivo (apaga uma pasta inteira).'],
  ['apagar', /\b(remove-item|ri|rm|rmdir|rd|del|erase)\b[^;|&\n]*\s-r(e(c(u(r(s(e)?)?)?)?)?)?\b/, 'Remove-Item -Recurse (apaga uma pasta inteira).'],
  ['apagar', /\b(del|erase|rd|rmdir)\b[^;|&\n]*\s\/s\b/, 'del/rd /s (apaga em massa).'],
  ['apagar', /(^|[\s;&|(])format(\.com)?\s+[a-z]:/, 'format (apaga um disco).'],
  ['apagar', /\b(diskpart|format-volume|clear-disk|remove-partition|initialize-disk)\b/, 'mexe nos discos e partições.'],
  ['apagar', /\bcipher(\.exe)?\s+[^;|&\n]*\/w\b/, 'cipher /w (apaga o espaço livre).'],
  ['apagar', /\bclear-recyclebin\b|\brd\s+\/s\s+[^\n]*\$recycle\.bin/, 'esvazia a Lixeira.'],
  ['apagar', /\bgit\s+clean\b[^;|&\n]*\s-[a-z]*f/, 'git clean -f (apaga arquivos não versionados).'],
  ['apagar', /\bgit\s+reset\b[^;|&\n]*--hard\b/, 'git reset --hard (descarta mudanças).'],
  ['apagar', /\bgit\s+push\b[^;|&\n]*(\s--force(-with-lease)?\b|\s-f\b|\s\+[^\s]+)/, 'git push forçado (reescreve o histórico no GitHub).'],
  ['apagar', /\b(vssadmin|wbadmin)\b[^;|&\n]*\bdelete\b/, 'apaga cópias de sombra ou backups.'],
  ['apagar', /shutil\.rmtree|\brmsync\s*\([^)]*recursive|\brimraf\b|fs\.rm\s*\([^)]*recursive/, 'apaga uma pasta inteira por script.'],
  // 2. sistema
  ['sistema', /\breg(\.exe)?\s+(add|delete|import|load|unload|restore|copy)\b[^\n]*\b(hklm|hkey_local_machine|hkcr|hkey_classes_root)\b/, 'muda o registro da máquina (HKLM/HKCR).'],
  ['sistema', /\b(set-itemproperty|new-itemproperty|remove-itemproperty|new-item|remove-item|rename-itemproperty|set-item)\b[^\n]*\b(hklm:|hkcr:|registry::hkey_(local_machine|classes_root))/, 'muda o registro da máquina (HKLM/HKCR).'],
  ['sistema', /\b(bcdedit|dism|sfc|bootrec|bcdboot)(\.exe)?\b/, 'mexe na inicialização ou na imagem do Windows.'],
  ['sistema', /\bsc(\.exe)?\s+(stop|start|config|delete|create|failure|pause)\b/, 'mexe em serviço do Windows.'],
  ['sistema', /\b(stop-service|start-service|restart-service|set-service|suspend-service|remove-service)\b/, 'mexe em serviço do Windows.'],
  ['sistema', /\b(pnputil|devcon|add-windowsdriver|fltmc)\b/, 'mexe em drivers.'],
  ['sistema', /(^|[\s;&|(])shutdown(\.exe)?\b|\b(restart-computer|stop-computer|logoff)\b/, 'desliga ou reinicia o PC.'],
  ['sistema', /-verb\s+runas\b|(^|[\s;&|(])(runas|gsudo|sudo)(\.exe)?\s/, 'pede elevação (administrador).'],
  ['sistema', /\bnetsh\b|\b(new|set|remove|disable|enable)-net(firewall\w*|adapter\w*|ipaddress|route|ipinterface)\b/, 'mexe na rede ou no firewall.'],
  ['sistema', /\b(copy-item|move-item|set-content|add-content|out-file|new-item|rename-item|cp|mv|copy|move|xcopy|robocopy|icacls|takeown|attrib|mkdir|md)\b[^\n]*[a-z]:\\(windows|program files|programdata)\b/, 'escreve numa pasta do sistema.'],
  ['sistema', />>?\s*[a-z]:\\(windows|program files|programdata)\b/, 'escreve numa pasta do sistema.'],
  // 3. segurança
  ['seguranca', /\b(set|add|remove)-mppreference\b|\bmpcmdrun\b/, 'mexe no Windows Defender (exclusões ou proteção).'],
  ['seguranca', /windows defender|\\defender\\|disableantispyware|disablerealtimemonitoring/, 'mexe no Windows Defender.'],
  ['seguranca', /verifiedandreputablepolicystate|smart ?app ?control|\\ci\\policy/, 'mexe no Smart App Control.'],
  ['seguranca', /\bwuauserv\b|\busoclient\b|windowsupdate|windows update/, 'mexe no Windows Update.'],
  ['seguranca', /\benablelua\b|consentpromptbehavior|promptonsecuredesktop/, 'mexe no UAC.'],
  ['seguranca', /\bset-executionpolicy\b(?![^\n;]*-scope\s+(process|currentuser)\b)/, 'Set-ExecutionPolicy para a máquina inteira.'],
  // 4. segredos e coleta
  ['segredos', /\bcmdkey(\.exe)?\s+\/list\b|\bvaultcmd\b|\bcred(read|enumerate)\w*\b|get-storedcredential|passwordvault/, 'lê o Cofre do Windows.'],
  ['segredos', /\b(getasynckeystate|setwindowshookex\w*|getkeyboardstate)\b/, 'padrão de keylogger (lê o teclado em segundo plano).'],
  ['segredos', /\b(writeprocessmemory|createremotethread|virtualallocex|ntcreatethreadex|queueuserapc|ntwritevirtualmemory)\b/, 'padrão de injeção em outro processo.'],
  ['segredos', /\b(while|for|do)\b[^\n]{0,200}(get-clipboard|clipboard\]::gettext|getclipboarddata)/, 'lê a área de transferência em laço.'],
  ['segredos', /(get-clipboard|clipboard\]::gettext)[^\n]{0,200}\b(start-sleep|sleep)\b/, 'lê a área de transferência em laço.'],
  ['segredos', /\b(login data|web data|local state)\b|\\cookies\b|logins\.json|key4\.db/, 'dados do navegador (senhas, cookies).'],
  ['segredos', /(^|[\s\\/=<>|(])\.env(\.(local|production|development|prod|dev|test))?(\s|$|[;|&)])/, 'arquivo .env (segredos).'],
  ['segredos', /\.pem\b|\bid_(rsa|dsa|ecdsa|ed25519)\b|github-recovery-codes|taylor-config-pessoal|pontedecomando\\token\b/, 'chave ou segredo.'],
  ['segredos', /\b(mimikatz|lsass|sekurlsa|procdump)\b|\bsam\b[^\n]*\breg\s+save|reg(\.exe)?\s+save\s+hklm\\(sam|security|system)/, 'coleta de credenciais do Windows.'],
  // 5. mandar para fora
  // curl: -F (formulário) e -T (upload) são maiúsculos; -f e -t minúsculos são outra coisa, então esta olha o texto cru
  ['envio', /\bcurl(\.exe)?\b[^\n]*(\s-[a-zA-Z]*[FT]\b|\s--form\b|\s--upload-file\b|--data(-binary|-raw|-urlencode)?[\s=]+@|\s-d\s*@)/, 'curl enviando arquivo para fora.', true],
  ['envio', /\b(invoke-webrequest|invoke-restmethod|iwr|irm)\b[^\n]*(-infile\b|-method\s+(post|put|patch)\b)/, 'envia dados para a internet (POST/PUT ou arquivo).'],
  ['envio', /(^|[\s;&|(])(scp|rsync|sftp|ftp|pscp|winscp)(\.exe)?\s/, 'copia para outra máquina (scp/rsync/ftp).'],
  ['envio', /\bgh\s+gist\s+create\b|\bgh\s+repo\s+create\b[^\n]*--public\b|\bgh\s+repo\s+edit\b[^\n]*--visibility\s+public|\bgh\s+api\b[^\n]*private[^\n]*false/, 'publica no GitHub (gist ou repositório público).'],
  ['envio', /\bnpm\s+publish\b|\bcargo\s+publish\b|\btwine\s+upload\b/, 'publica um pacote.'],
  ['envio', /\bsend-mailmessage\b|\bsmtpclient\b|\bsmtplib\b|\bmailto:|\bblat(\.exe)?\b/, 'manda e-mail.'],
  ['envio', /requests\.(post|put|patch)\s*\(|urllib\.request\.urlopen\s*\([^)]*data=|\bfetch\s*\([^)]*method\s*:\s*(post|put)/, 'script enviando dados para a internet.'],
  ['envio', /(^|[\s;&|(])(nc|ncat|netcat)(\.exe)?\s/, 'netcat (conexão crua para fora).'],
  // 6. persistência
  ['persistencia', /\bschtasks(\.exe)?\s+[^\n]*\/create\b|\b(register-scheduledtask|new-scheduledtask)\b/, 'cria tarefa agendada.'],
  ['persistencia', /currentversion\\run(once)?\b/, 'chave Run/RunOnce (abrir com o Windows).'],
  ['persistencia', /start menu\\programs\\startup|shell:startup|shell:common startup/, 'pasta Inicializar.'],
  ['persistencia', /\bnew-service\b|\bsc(\.exe)?\s+create\b/, 'cria serviço do Windows.'],
  ['persistencia', /\b(register-wmievent|set-wmiinstance|register-cimindicationevent)\b|__eventfilter|commandlineeventconsumer|__filtertoconsumerbinding/, 'assinatura WMI (persistência).'],
  // 7. baixado ou ofuscado
  ['baixado', /\b(iex|invoke-expression)\b/, 'Invoke-Expression (executa texto como código).'],
  ['baixado', /\b(curl|wget|iwr|irm|invoke-webrequest|invoke-restmethod)\b[^\n]*\|\s*(sh|bash|zsh|python\d?|node|pwsh|powershell|cmd)\b/, 'baixa e executa direto (| sh).'],
  ['baixado', /\b(powershell|pwsh)(\.exe)?\b[^\n]*\s-(e|ec|en|enc|enco|encod|encode|encoded|encodedc\w*)\s+[a-z0-9+/=]{8,}/, 'PowerShell com comando codificado (-EncodedCommand).'],
  ['baixado', /frombase64string|\bbase64\s+(-d|--decode)\b[^\n]*\|\s*(sh|bash|python|node)|atob\s*\([^)]*\)\s*\)|\bcertutil\b[^\n]*-decode/, 'decodifica base64 para executar.'],
  ['baixado', /\[reflection\.assembly\]::load|\[scriptblock\]::create|add-type\b[^\n]*-path\s+[^\n]*\\(downloads|temp)\\/, 'carrega código dinâmico.'],
  ['baixado', /\\(downloads|temp|tmp)\\[^\s;|&]*\.(exe|msi|scr|com|bat|cmd|vbs|hta)\b|\$env:temp[^\s;|&]*\.(exe|msi|scr)\b|%temp%[^\s;|&]*\.(exe|msi|scr)\b|\bmshta\b|\bregsvr32\b[^\n]*\/i:http|\brundll32\b[^\n]*(javascript|http)/, 'roda executável vindo de Downloads ou Temp.'],
  // 8. áreas protegidas do Alex
  // "valores" sozinho é palavra comum: só conta como o programa com .exe, num comando de processo ou na pasta de dados
  ['protegido', /\bvalores\.exe\b|\b(stop-process|taskkill|get-process|kill|spps)\b[^\n]*\bvalores\b|com\.pixelmartins\.valores/, 'o Valores (programa e dados do Alex).'],
  ['protegido', /livro-sangue-sintetico|sangue-sintetico-leitores-beta/, 'o livro do Alex.'],
  ['protegido', /conversas-com-claude/, 'o diário do Alex.'],
  ['protegido', /\.claude[\\/]settings(\.local)?\.json/, 'o settings.json do Claude Code.'],
]

/** Sinais de que a guarda não consegue ler o script com confiança: vai para o cartão. */
function ofuscado(raw, n) {
  if (raw.length > MAX_COMANDO) return 'comando grande demais para analisar'
  if ((n.match(/\[char\]/g) || []).length >= 4) return 'texto montado com [char] (ofuscado)'
  if (/-join\s*\(?\s*\(?\s*\[char/.test(n) || /-bxor\b/.test(n)) return 'texto montado ou cifrado em tempo de execução'
  if (/\w`\w`\w/.test(raw)) return 'crases no meio das palavras (ofuscado)'
  if (/\w\^\w\^\w/.test(raw) && !/\b(grep|rg|sed|awk|select-string|findstr|-match|-replace|regex)\b/i.test(raw)) return 'circunflexos no meio das palavras (ofuscado)'
  if (/[a-z0-9+/]{300,}={0,2}/i.test(raw.replace(/\s/g, '')) && /(-e\w*\s|base64|frombase64|decode)/i.test(raw)) return 'bloco grande em base64'
  if (/\$\{?\w+\}?\s*=\s*['"][^'"]{0,40}['"]\s*\+\s*['"][^'"]{0,40}['"][^\n]*&\s*\(?\s*\$/i.test(raw)) return 'nome de comando montado por pedaços e executado'
  return null
}

const VERBOS_APAGAR = /(^|[\s;&|(])(remove-item|ri|rm|rmdir|rd|del|erase|unlink|rimraf)(\.exe)?\s+([^\n;&|]*)/g

/** Apagar (mesmo sem -Recurse) fora da pasta de trabalho, com curinga ou com "..". */
function apagarFora(raw, pasta) {
  const t = raw.replace(/["']/g, ' ')
  let m
  VERBOS_APAGAR.lastIndex = 0
  while ((m = VERBOS_APAGAR.exec(t.toLowerCase()))) {
    const args = m[4].split(/\s+/).filter((a) => a && !a.startsWith('-') && !/^\/[a-z]$/i.test(a))
    for (const a of args) {
      if (a.includes('*') && /[\\/]/.test(a.replace(/^\.[\\/]/, ''))) {
        const base = a.slice(0, a.lastIndexOf('*'))
        if (!base || !dentro(absoluto(base) ? base : pasta + '\\' + base, pasta)) return 'apaga com curinga fora da pasta de trabalho.'
      }
      if (/(^|[\\/])\.\.([\\/]|$)/.test(a)) return 'apaga subindo de pasta (..).'
      if (/^(~|\$env:|\$home|%[a-z]+%)/.test(a)) return 'apaga na pasta do usuário ou do sistema.'
      if (absoluto(a) || /^\/[a-z]\//.test(a)) {
        if (!dentro(a, pasta)) return 'apaga fora da pasta de trabalho.'
      }
    }
  }
  return null
}

function perguntar(categoria, motivo) {
  return { decisao: 'perguntar', categoria, rotulo: CATEGORIAS[categoria], motivo }
}
const LIBERAR = Object.freeze({ decisao: 'liberar' })

/**
 * Decide um pedido de permissão.
 * @param {string} ferramenta  nome da ferramenta do Claude Code (Bash, PowerShell, Write, Read…)
 * @param {object} entrada     a entrada da ferramenta
 * @param {{ pastaTrabalho: string }} ctx
 * @returns {{ decisao: 'liberar' } | { decisao: 'perguntar', categoria, rotulo, motivo }}
 */
export function avaliar(ferramenta, entrada, ctx = {}) {
  const pasta = ctx.pastaTrabalho || 'A:\\Claude'
  const e = entrada && typeof entrada === 'object' ? entrada : {}
  try {
    if (SHELL.has(ferramenta)) {
      const raw = typeof e.command === 'string' ? e.command : null
      if (raw === null) return perguntar('desconhecido', 'O comando veio num formato que a guarda não lê.')
      const n = normal(raw)
      const ruim = ofuscado(raw, n)
      if (ruim) return perguntar('desconhecido', `Script que a guarda não consegue ler com segurança: ${ruim}.`)
      const nc = n.replace(/\//g, '\\') // também com barras invertidas, para as regras de caminho
      const cru = raw.replace(/["'`^]/g, '')
      for (const [cat, re, motivo, bruto] of REGRAS) {
        const bate = bruto ? re.test(cru) : re.test(n) || re.test(nc)
        if (bate) return perguntar(cat, motivo[0].toUpperCase() + motivo.slice(1))
      }
      const fora = apagarFora(raw, pasta)
      if (fora) return perguntar('apagar', fora[0].toUpperCase() + fora.slice(1))
      return LIBERAR
    }
    if (SO_LEITURA.has(ferramenta)) {
      for (const k of ['file_path', 'path', 'notebook_path']) if (typeof e[k] === 'string') { const r = olharCaminho(e[k], false, pasta); if (r) return r }
      if (typeof e.pattern === 'string' && ferramenta === 'Glob') { const r = olharCaminho(e.pattern, false, pasta); if (r) return r }
      return LIBERAR
    }
    if (ESCRITA.has(ferramenta)) {
      const alvo = e.file_path || e.notebook_path || e.path
      if (typeof alvo !== 'string' || !alvo.trim()) return perguntar('desconhecido', 'Escrita sem caminho que a guarda consiga ler.')
      return olharCaminho(alvo, true, pasta) || LIBERAR
    }
    if (INOFENSIVAS.has(ferramenta)) return LIBERAR
    if (String(ferramenta).startsWith('mcp__')) return perguntar('desconhecido', 'Ferramenta de conector (MCP): pode mandar dados para fora; a guarda não analisa.')
    return perguntar('desconhecido', `Ferramenta que a guarda não conhece: ${String(ferramenta).slice(0, 60)}.`)
  } catch (err) {
    return perguntar('desconhecido', 'A guarda falhou ao analisar; na dúvida, pergunta.')
  }
}

// ---------------------------------------------------------------- registro (sem segredo)

/** Resumo de até 140 letras para o registro, sem tokens, chaves, senhas nem blocos longos. */
export function resumo(ferramenta, entrada) {
  const e = entrada && typeof entrada === 'object' ? entrada : {}
  let t = e.command || e.file_path || e.notebook_path || e.path || e.pattern || e.url || e.query || e.description || ''
  t = String(t)
    .replace(/sk-ant-[A-Za-z0-9_-]+/g, '[chave]')
    .replace(/\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{10,}/g, '[token]')
    .replace(/github_pat_[A-Za-z0-9_]+/g, '[token]')
    .replace(/\b(bearer|basic)\s+[^\s"']+/gi, '$1 [oculto]')
    .replace(/(authorization|token|senha|password|passwd|secret|api[-_]?key)(\s*[:=]\s*|\s+)[^\s"']+/gi, '$1$2[oculto]')
    .replace(/[A-Za-z0-9+/=_-]{40,}/g, '[…]')
    .replace(/\s+/g, ' ')
    .trim()
  if (t.length > 140) t = t.slice(0, 139) + '…'
  return `${ferramenta}: ${t}`.slice(0, 160)
}
