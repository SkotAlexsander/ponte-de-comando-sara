//! Conversas com o Claude Code abertas pelo programa.
//! Cada conversa da interface tem a SUA sessão do `claude`, com o seu modelo, esforço e velocidade:
//!   1ª mensagem:  claude -p --output-format stream-json --verbose --model M --effort E
//!                        --settings {"fastMode":R} --session-id <uuid novo>
//!   as seguintes: o mesmo, com --resume <uuid> no lugar de --session-id
//! O texto vai pelo stdin (nada de texto do Alex na linha de comando) e a saída em JSON por linha
//! vira eventos `ponte:conversa` para as janelas.

use crate::config::{agora_ms, gravar_atomico, uuid_v4};
use crate::estado::{cortar, Evento};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Pedido {
    /// Identificador da conversa na interface (ex.: "landing@k3f9"). Letras, números, - _ . @ :
    pub conversa: String,
    pub texto: String,
    /// "opus" | "sonnet" | "haiku" (aceita também "OPUS" etc. e um id completo "claude-…")
    pub modelo: String,
    /// "baixo" | "médio" | "alto" | "máximo" (ou low/medium/high/xhigh/max)
    pub esforco: String,
    /// Modo rápido do Claude Code.
    pub rapido: Option<bool>,
    /// Alternativa a `rapido`: "NORMAL" | "RÁPIDA".
    pub velocidade: String,
    /// Pasta onde a sessão roda, relativa à pasta de trabalho ("" = a própria). Só vale na 1ª mensagem.
    pub pasta: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InfoConversa {
    pub sessao: String,
    pub iniciada: bool,
    pub modelo: String,
    pub esforco: String,
    pub rapido: bool,
    pub pasta: String,
    pub criada_em: u64,
    pub atualizada_em: u64,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventoConversa {
    pub conversa: String,
    /// "inicio" | "texto" | "ferramenta" | "resultado" | "erro" | "cancelado"
    pub tipo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texto: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legenda: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detalhe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessao: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modelo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erro: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custo: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duracao_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_entrada: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_saida: Option<u64>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Resposta {
    pub conversa: String,
    pub sessao: String,
    pub nova: bool,
    pub modelo: String,
    pub esforco: String,
    pub rapido: bool,
    pub pasta: String,
    /// "sdk" (sidecar com o Claude Agent SDK) ou "cli" (claude -p, sem cartão de permissão)
    pub motor: &'static str,
}

struct Rodando {
    filho: Mutex<Option<Child>>,
    cancelado: AtomicBool,
}

pub struct Conversas {
    arquivo: PathBuf,
    mapa: Mutex<HashMap<String, InfoConversa>>,
    rodando: Mutex<HashMap<String, Arc<Rodando>>>,
    /// Conversas em andamento pelo sidecar (SDK).
    rodando_sdk: Mutex<std::collections::HashSet<String>>,
}

pub type Saida = Arc<dyn Fn(EventoConversa) + Send + Sync>;
pub type AoEstado = Arc<dyn Fn(Evento) + Send + Sync>;

pub fn normalizar_modelo(m: &str) -> Result<String, String> {
    let l = m.trim().to_lowercase();
    match l.as_str() {
        "" | "opus" => Ok("opus".into()),
        "sonnet" => Ok("sonnet".into()),
        "haiku" => Ok("haiku".into()),
        x if x.starts_with("claude-") && x.len() < 80 && x.chars().all(|c| c.is_ascii_alphanumeric() || "-.[]".contains(c)) => Ok(x.into()),
        _ => Err(format!("Modelo desconhecido: {m}")),
    }
}

pub fn normalizar_esforco(e: &str) -> Result<String, String> {
    let l = e.trim().to_lowercase();
    match l.as_str() {
        "baixo" | "low" => Ok("low".into()),
        "médio" | "medio" | "medium" => Ok("medium".into()),
        "" | "alto" | "high" => Ok("high".into()),
        "extra" | "xhigh" | "muito alto" => Ok("xhigh".into()),
        "máximo" | "maximo" | "max" => Ok("max".into()),
        _ => Err(format!("Esforço desconhecido: {e}")),
    }
}

pub fn normalizar_rapido(p: &Pedido) -> bool {
    if let Some(r) = p.rapido {
        return r;
    }
    matches!(p.velocidade.trim().to_lowercase().as_str(), "rápida" | "rapida" | "rápido" | "rapido" | "fast")
}

fn id_valido(id: &str) -> bool {
    !id.is_empty() && id.len() <= 120 && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.@:".contains(c))
}

/// Argumentos do `claude` para esta mensagem (sem o texto, que vai pelo stdin).
pub fn montar_args(info: &InfoConversa) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--model".into(),
        info.modelo.clone(),
        "--effort".into(),
        info.esforco.clone(),
        "--settings".into(),
        serde_json::json!({ "fastMode": info.rapido }).to_string(),
        // nunca agir sem aprovação: no modo -p o que não estiver liberado é negado
        "--permission-mode".into(),
        "default".into(),
    ];
    if info.iniciada {
        a.push("--resume".into());
    } else {
        a.push("--session-id".into());
    }
    a.push(info.sessao.clone());
    a
}

fn nome_arquivo(v: &Value, chave: &str) -> String {
    v.get(chave)
        .and_then(|x| x.as_str())
        .map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string())
        .unwrap_or_default()
}

/// Linha curta em PT-BR para uma ferramenta do Claude Code (igual à do hook).
pub fn legenda_ferramenta(nome: &str, entrada: &Value) -> (String, String) {
    let s = |k: &str| entrada.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let (leg, det) = match nome {
        "Read" | "NotebookRead" => {
            let f = nome_arquivo(entrada, "file_path");
            (format!("Lendo {f}"), f)
        }
        "Write" => {
            let f = nome_arquivo(entrada, "file_path");
            (format!("Escrevendo {f}"), f)
        }
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            let f = nome_arquivo(entrada, if nome == "NotebookEdit" { "notebook_path" } else { "file_path" });
            (format!("Editando {f}"), f)
        }
        "Bash" | "PowerShell" => {
            let d = s("description");
            let d = if d.is_empty() { s("command") } else { d };
            (format!("Rodando: {}", cortar(&d, 60)), cortar(&d, 80))
        }
        "Grep" => (format!("Procurando \"{}\"", cortar(&s("pattern"), 40)), s("pattern")),
        "Glob" => (format!("Procurando arquivos {}", cortar(&s("pattern"), 40)), s("pattern")),
        "Agent" | "Task" => {
            let quem = s("subagent_type");
            let d = s("description");
            let quem = if quem.is_empty() { "um funcionário".to_string() } else { quem };
            (format!("Chamando {quem}"), cortar(&d, 80))
        }
        "WebFetch" => {
            let u = s("url");
            let host = u.split("//").nth(1).unwrap_or(&u).split('/').next().unwrap_or("").to_string();
            (format!("Lendo a página {host}"), host)
        }
        "WebSearch" => (format!("Pesquisando \"{}\"", cortar(&s("query"), 40)), s("query")),
        "TodoWrite" => ("Organizando as tarefas".into(), String::new()),
        "Skill" => (format!("Usando a skill {}", s("skill")), s("skill")),
        n if n.starts_with("mcp__") => {
            let curto = n.rsplit("__").next().unwrap_or(n).to_string();
            (format!("Usando {curto}"), String::new())
        }
        n => (format!("Usando {n}"), String::new()),
    };
    (cortar(&leg, 90), cortar(&det, 100))
}

/// Traduz uma linha do stream-json em eventos para a interface.
pub fn traduzir_linha(conversa: &str, linha: &str) -> Vec<EventoConversa> {
    let v: Value = match serde_json::from_str(linha) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let base = || EventoConversa { conversa: conversa.to_string(), ..Default::default() };
    let tipo = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match tipo {
        "system" if v.get("subtype").and_then(|x| x.as_str()) == Some("init") => vec![EventoConversa {
            tipo: "inicio".into(),
            sessao: v.get("session_id").and_then(|x| x.as_str()).map(String::from),
            modelo: v.get("model").and_then(|x| x.as_str()).map(String::from),
            ..base()
        }],
        "assistant" => {
            let mut saida = vec![];
            if let Some(blocos) = v.pointer("/message/content").and_then(|x| x.as_array()) {
                for b in blocos {
                    match b.get("type").and_then(|x| x.as_str()) {
                        Some("text") => {
                            let t = b.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                            if !t.is_empty() {
                                saida.push(EventoConversa { tipo: "texto".into(), texto: Some(t), ..base() });
                            }
                        }
                        Some("tool_use") => {
                            let nome = b.get("name").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                            let (leg, det) = legenda_ferramenta(&nome, b.get("input").unwrap_or(&Value::Null));
                            saida.push(EventoConversa { tipo: "ferramenta".into(), nome: Some(nome), legenda: Some(leg), detalhe: Some(det), ..base() });
                        }
                        _ => {}
                    }
                }
            }
            saida
        }
        "result" => {
            let erro = v.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false)
                || v.get("subtype").and_then(|x| x.as_str()).map(|s| s != "success").unwrap_or(false);
            vec![EventoConversa {
                tipo: "resultado".into(),
                texto: v.get("result").and_then(|x| x.as_str()).map(String::from),
                erro: Some(erro),
                sessao: v.get("session_id").and_then(|x| x.as_str()).map(String::from),
                custo: v.get("total_cost_usd").and_then(|x| x.as_f64()),
                duracao_ms: v.get("duration_ms").and_then(|x| x.as_u64()),
                tokens_entrada: v.pointer("/usage/input_tokens").and_then(|x| x.as_u64()),
                tokens_saida: v.pointer("/usage/output_tokens").and_then(|x| x.as_u64()),
                ..base()
            }]
        }
        _ => vec![],
    }
}

fn exe_claude() -> (PathBuf, Vec<String>) {
    if let Ok(e) = std::env::var("PONTE_CLAUDE") {
        let prefixo = std::env::var("PONTE_CLAUDE_PREFIXO").map(|p| vec![p]).unwrap_or_default();
        return (PathBuf::from(e), prefixo);
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = Path::new(&home).join(".local").join("bin").join("claude.exe");
        if p.is_file() {
            return (p, vec![]);
        }
    }
    (PathBuf::from("claude"), vec![])
}

impl Conversas {
    pub fn abrir(pasta_dados: &Path) -> Arc<Conversas> {
        let arquivo = pasta_dados.join("conversas.json");
        let mapa = std::fs::read_to_string(&arquivo).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Arc::new(Conversas { arquivo, mapa: Mutex::new(mapa), rodando: Mutex::new(HashMap::new()), rodando_sdk: Mutex::new(Default::default()) })
    }

    fn salvar(&self) {
        let mapa = self.mapa.lock().unwrap();
        if let Ok(t) = serde_json::to_string_pretty(&*mapa) {
            let _ = gravar_atomico(&self.arquivo, t.as_bytes());
        }
    }

    pub fn info(&self, conversa: &str) -> Option<InfoConversa> {
        self.mapa.lock().unwrap().get(conversa).cloned()
    }

    /// Esta sessão do Claude Code foi aberta pelo programa?
    pub fn tem_sessao(&self, sessao: &str) -> bool {
        self.mapa.lock().unwrap().values().any(|i| i.sessao == sessao)
    }

    pub fn rodando(&self, conversa: &str) -> bool {
        self.rodando.lock().unwrap().contains_key(conversa) || self.rodando_sdk.lock().unwrap().contains(conversa)
    }

    pub fn nenhuma_rodando(&self) -> bool {
        self.rodando.lock().unwrap().is_empty() && self.rodando_sdk.lock().unwrap().is_empty()
    }

    pub fn marcar_rodando_sdk(&self, conversa: &str, sim: bool) {
        let mut g = self.rodando_sdk.lock().unwrap();
        if sim {
            g.insert(conversa.to_string());
        } else {
            g.remove(conversa);
        }
    }

    pub fn limpar_rodando_sdk(&self) -> Vec<String> {
        self.rodando_sdk.lock().unwrap().drain().collect()
    }

    /// Prepara a conversa (cria ou atualiza modelo/esforço/velocidade) sem rodar nada.
    pub fn preparar(&self, p: &Pedido, pasta_rel: String) -> Result<InfoConversa, String> {
        if !id_valido(&p.conversa) {
            return Err("Identificador de conversa inválido.".into());
        }
        let modelo = normalizar_modelo(&p.modelo)?;
        let esforco = normalizar_esforco(&p.esforco)?;
        let rapido = normalizar_rapido(p);
        let mut mapa = self.mapa.lock().unwrap();
        if mapa.len() > 500 {
            // guarda as 300 mais recentes
            let mut v: Vec<_> = mapa.iter().map(|(k, i)| (k.clone(), i.atualizada_em)).collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.1));
            for (k, _) in v.into_iter().skip(300) {
                mapa.remove(&k);
            }
        }
        let agora = agora_ms();
        let info = mapa.entry(p.conversa.clone()).or_insert_with(|| InfoConversa {
            sessao: uuid_v4(),
            iniciada: false,
            modelo: modelo.clone(),
            esforco: esforco.clone(),
            rapido,
            pasta: pasta_rel,
            criada_em: agora,
            atualizada_em: agora,
        });
        info.modelo = modelo;
        info.esforco = esforco;
        info.rapido = rapido;
        info.atualizada_em = agora;
        let copia = info.clone();
        drop(mapa);
        self.salvar();
        Ok(copia)
    }

    /// Liga uma conversa da tela a uma sessão do Claude Code que já existe (retomar conversa antiga).
    pub fn ligar_sessao(&self, conversa: &str, sessao: &str, pasta: &str) -> Result<InfoConversa, String> {
        if !id_valido(conversa) {
            return Err("Identificador de conversa inválido.".into());
        }
        let uuid_ok = sessao.len() == 36 && sessao.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if !uuid_ok {
            return Err("Identificador de sessão inválido.".into());
        }
        if self.rodando(conversa) {
            return Err("Essa conversa está trabalhando agora.".into());
        }
        let agora = agora_ms();
        let info = {
            let mut mapa = self.mapa.lock().unwrap();
            let anterior = mapa.get(conversa).cloned();
            let info = InfoConversa {
                sessao: sessao.to_string(),
                iniciada: true,
                modelo: anterior.as_ref().map(|i| i.modelo.clone()).unwrap_or_else(|| "opus".into()),
                esforco: anterior.as_ref().map(|i| i.esforco.clone()).unwrap_or_else(|| "high".into()),
                rapido: anterior.as_ref().map(|i| i.rapido).unwrap_or(false),
                pasta: pasta.to_string(),
                criada_em: agora,
                atualizada_em: agora,
            };
            mapa.insert(conversa.to_string(), info.clone());
            info
        };
        self.salvar();
        Ok(info)
    }

    pub fn marcar_iniciada(&self, conversa: &str) {
        let mut mudou = false;
        if let Some(i) = self.mapa.lock().unwrap().get_mut(conversa) {
            if !i.iniciada {
                i.iniciada = true;
                mudou = true;
            }
        }
        if mudou {
            self.salvar();
        }
    }

    pub fn cancelar(&self, conversa: &str) -> bool {
        let r = self.rodando.lock().unwrap().get(conversa).cloned();
        if let Some(r) = r {
            r.cancelado.store(true, Ordering::SeqCst);
            if let Some(f) = r.filho.lock().unwrap().as_mut() {
                let _ = f.kill();
            }
            true
        } else {
            false
        }
    }

    pub fn cancelar_todas(&self) {
        let ids: Vec<String> = self.rodando.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.cancelar(&id);
        }
    }

    /// Envia uma mensagem. Volta logo; o resto chega por `saida` (e o estado geral por `ao_estado`).
    pub fn enviar(self: &Arc<Self>, p: Pedido, raiz: &Path, saida: Saida, ao_estado: AoEstado) -> Result<Resposta, String> {
        let texto = p.texto.trim().to_string();
        if texto.is_empty() {
            return Err("Escreva a mensagem.".into());
        }
        if texto.len() > 200_000 {
            return Err("Mensagem grande demais.".into());
        }
        if self.rodando(&p.conversa) {
            return Err("Essa conversa ainda está trabalhando. Espere terminar ou aperte PARAR.".into());
        }
        // pasta da sessão: só na 1ª mensagem; depois fica a gravada
        let ja = self.info(&p.conversa);
        let pasta_rel = match &ja {
            Some(i) => i.pasta.clone(),
            None => {
                let c = crate::arquivos::resolver(raiz, &p.pasta)?;
                crate::arquivos::exibir(&c)
            }
        };
        let info = self.preparar(&p, pasta_rel)?;
        let cwd = PathBuf::from(&info.pasta);
        if !cwd.is_dir() {
            return Err(format!("A pasta desta conversa não existe mais: {}", info.pasta));
        }
        let (exe, prefixo) = exe_claude();
        let mut cmd = Command::new(&exe);
        cmd.args(&prefixo)
            .args(montar_args(&info))
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_ENTRYPOINT");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut filho = cmd.spawn().map_err(|e| format!("Não consegui abrir o Claude Code ({}): {e}", exe.display()))?;
        if let Some(mut entrada) = filho.stdin.take() {
            let t = texto.clone();
            std::thread::spawn(move || {
                let _ = entrada.write_all(t.as_bytes());
            });
        }
        let stdout = filho.stdout.take();
        let stderr = filho.stderr.take();
        let rod = Arc::new(Rodando { filho: Mutex::new(Some(filho)), cancelado: AtomicBool::new(false) });
        self.rodando.lock().unwrap().insert(p.conversa.clone(), rod.clone());

        let projeto = cwd.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        let titulo = cortar(&texto, 140);
        ao_estado(Evento { tipo: "trabalhando".into(), fase: "pedido".into(), sessao: info.sessao.clone(), projeto: projeto.clone(), titulo: titulo.clone(), origem: "programa".into(), ..Default::default() });

        let resultado_visto = Arc::new(AtomicBool::new(false));
        let cauda_erro = Arc::new(Mutex::new(String::new()));

        // stderr: guarda o fim para explicar um erro
        let leitor_erro = stderr.map(|mut e| {
            let cauda = cauda_erro.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = e.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut c = cauda.lock().unwrap();
                    c.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if c.len() > 4000 {
                        let corte = c.len() - 4000;
                        let corte = (corte..c.len()).find(|i| c.is_char_boundary(*i)).unwrap_or(0);
                        *c = c[corte..].to_string();
                    }
                }
            })
        });

        // stdout: JSON por linha
        let leitor_saida = stdout.map(|o| {
            let saida = saida.clone();
            let ao = ao_estado.clone();
            let eu = self.clone();
            let conversa = p.conversa.clone();
            let visto = resultado_visto.clone();
            let sessao = info.sessao.clone();
            let projeto = projeto.clone();
            let titulo = titulo.clone();
            std::thread::spawn(move || {
                for linha in BufReader::new(o).lines() {
                    let Ok(linha) = linha else { break };
                    for ev in traduzir_linha(&conversa, &linha) {
                        match ev.tipo.as_str() {
                            "inicio" => eu.marcar_iniciada(&conversa),
                            "ferramenta" => ao(Evento {
                                tipo: "trabalhando".into(),
                                fase: "ferramenta".into(),
                                sessao: sessao.clone(),
                                projeto: projeto.clone(),
                                ferramenta: ev.nome.clone().unwrap_or_default(),
                                legenda: ev.legenda.clone().unwrap_or_default(),
                                origem: "programa".into(),
                                ..Default::default()
                            }),
                            "resultado" => {
                                visto.store(true, Ordering::SeqCst);
                                let erro = ev.erro.unwrap_or(false);
                                ao(Evento {
                                    tipo: if erro { "erro".into() } else { "pronto".into() },
                                    sessao: sessao.clone(),
                                    projeto: projeto.clone(),
                                    titulo: if erro { String::new() } else { titulo.clone() },
                                    texto: if erro { ev.texto.clone().unwrap_or_default() } else { String::new() },
                                    resumo: ev.texto.clone().unwrap_or_default(),
                                    origem: "programa".into(),
                                    ..Default::default()
                                });
                            }
                            _ => {}
                        }
                        saida(ev);
                    }
                }
            })
        });

        // espera o fim sem prender o filho (para o PARAR conseguir matar)
        {
            let eu = self.clone();
            let conversa = p.conversa.clone();
            std::thread::spawn(move || {
                let codigo = loop {
                    let fim = {
                        let mut g = rod.filho.lock().unwrap();
                        match g.as_mut().map(|f| f.try_wait()) {
                            Some(Ok(Some(st))) => Some(st.code()),
                            Some(Ok(None)) => None,
                            _ => Some(None),
                        }
                    };
                    if let Some(c) = fim {
                        break c;
                    }
                    std::thread::sleep(Duration::from_millis(150));
                };
                if let Some(h) = leitor_saida {
                    let _ = h.join();
                }
                if let Some(h) = leitor_erro {
                    let _ = h.join();
                }
                eu.rodando.lock().unwrap().remove(&conversa);
                if rod.cancelado.load(Ordering::SeqCst) {
                    saida(EventoConversa { conversa: conversa.clone(), tipo: "cancelado".into(), ..Default::default() });
                    ao_estado(Evento { tipo: "esperando".into(), origem: "programa".into(), ..Default::default() });
                } else if !resultado_visto.load(Ordering::SeqCst) {
                    let cauda = cauda_erro.lock().unwrap().trim().to_string();
                    let msg = if cauda.is_empty() {
                        format!("O Claude Code saiu sem responder (código {}).", codigo.map(|c| c.to_string()).unwrap_or("?".into()))
                    } else {
                        cortar(&cauda, 600)
                    };
                    saida(EventoConversa { conversa: conversa.clone(), tipo: "erro".into(), texto: Some(msg.clone()), ..Default::default() });
                    ao_estado(Evento { tipo: "erro".into(), texto: msg, origem: "programa".into(), ..Default::default() });
                }
            });
        }

        Ok(Resposta {
            conversa: p.conversa,
            nova: ja.map(|i| !i.iniciada).unwrap_or(true),
            sessao: info.sessao,
            modelo: info.modelo,
            esforco: info.esforco,
            rapido: info.rapido,
            pasta: info.pasta,
            motor: "cli",
        })
    }
}

pub fn validar_texto(t: &str) -> Result<String, String> {
    let t = t.trim().to_string();
    if t.is_empty() {
        return Err("Escreva a mensagem.".into());
    }
    if t.len() > 200_000 {
        return Err("Mensagem grande demais.".into());
    }
    Ok(t)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza() {
        assert_eq!(normalizar_modelo("OPUS").unwrap(), "opus");
        assert_eq!(normalizar_modelo("Haiku").unwrap(), "haiku");
        assert!(normalizar_modelo("gpt").is_err());
        assert!(normalizar_modelo("claude-x; rm").is_err());
        assert_eq!(normalizar_esforco("MÉDIO").unwrap(), "medium");
        assert_eq!(normalizar_esforco("MÁXIMO").unwrap(), "max");
        assert_eq!(normalizar_esforco("baixo").unwrap(), "low");
        assert!(normalizar_esforco("turbo").is_err());
        let mut p = Pedido::default();
        p.velocidade = "RÁPIDA".into();
        assert!(normalizar_rapido(&p));
        p.rapido = Some(false);
        assert!(!normalizar_rapido(&p));
    }

    #[test]
    fn argumentos_por_conversa() {
        let mut i = InfoConversa { sessao: "u-1".into(), iniciada: false, modelo: "sonnet".into(), esforco: "low".into(), rapido: true, pasta: "x".into(), criada_em: 0, atualizada_em: 0 };
        let a = montar_args(&i);
        assert!(a.windows(2).any(|w| w == ["--model", "sonnet"]));
        assert!(a.windows(2).any(|w| w == ["--effort", "low"]));
        assert!(a.windows(2).any(|w| w[0] == "--settings" && w[1] == r#"{"fastMode":true}"#));
        assert!(a.windows(2).any(|w| w == ["--session-id", "u-1"]));
        i.iniciada = true;
        assert!(montar_args(&i).windows(2).any(|w| w == ["--resume", "u-1"]));
    }

    #[test]
    fn cada_conversa_guarda_o_seu() {
        let d = std::env::temp_dir().join(format!("ponte-conv-{}", crate::config::aleatorio_hex(4)));
        std::fs::create_dir_all(&d).unwrap();
        let c = Conversas::abrir(&d);
        let a = c.preparar(&Pedido { conversa: "a@1".into(), modelo: "HAIKU".into(), esforco: "BAIXO".into(), ..Default::default() }, "x".into()).unwrap();
        let b = c.preparar(&Pedido { conversa: "b@1".into(), modelo: "OPUS".into(), esforco: "MÁXIMO".into(), velocidade: "RÁPIDA".into(), ..Default::default() }, "x".into()).unwrap();
        assert_ne!(a.sessao, b.sessao);
        assert_eq!((a.modelo.as_str(), a.esforco.as_str(), a.rapido), ("haiku", "low", false));
        assert_eq!((b.modelo.as_str(), b.esforco.as_str(), b.rapido), ("opus", "max", true));
        // trocar o modelo no meio mantém a sessão
        let a2 = c.preparar(&Pedido { conversa: "a@1".into(), modelo: "SONNET".into(), esforco: "ALTO".into(), ..Default::default() }, "y".into()).unwrap();
        assert_eq!(a2.sessao, a.sessao);
        assert_eq!(a2.modelo, "sonnet");
        assert_eq!(a2.pasta, "x");
        assert!(c.preparar(&Pedido { conversa: "a b".into(), ..Default::default() }, "x".into()).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn traduz_stream() {
        let ini = traduzir_linha("c", r#"{"type":"system","subtype":"init","session_id":"s","model":"claude-haiku-4-5"}"#);
        assert_eq!(ini[0].tipo, "inicio");
        let a = traduzir_linha("c", r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Oi"},{"type":"tool_use","name":"Read","input":{"file_path":"A:\\x\\LEIA-ME.md"}}]}}"#);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].texto.as_deref(), Some("Oi"));
        assert_eq!(a[1].legenda.as_deref(), Some("Lendo LEIA-ME.md"));
        let r = traduzir_linha("c", r#"{"type":"result","subtype":"success","is_error":false,"result":"Feito","total_cost_usd":0.01,"usage":{"input_tokens":5,"output_tokens":7}}"#);
        assert_eq!(r[0].tipo, "resultado");
        assert_eq!(r[0].erro, Some(false));
        assert_eq!(r[0].tokens_saida, Some(7));
        assert!(traduzir_linha("c", "lixo").is_empty());
    }
}
