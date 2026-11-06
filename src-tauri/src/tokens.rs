//! Motor de tokens, sessões vivas, limites do plano, relógio do cache, conta e conexões MCP.
//!
//! A regra de contagem é portada de `painel-tokens-claude-vscode/src/motor.js`
//! (MIT License, Copyright (c) 2026 Roger Fleming — ver NOTICE.md):
//! - lê só o pedaço novo de cada `.jsonl` (offset + resto de linha incompleta);
//! - uma resposta aparece em várias linhas com o mesmo `message.id`: fica a de MAIOR `output_tokens`,
//!   desfazendo a soma anterior;
//! - ignora o modelo `<synthetic>`;
//! - custo = input×entrada + cache_read×entrada×0,1 + escrita5m×entrada×1,25 + escrita1h×entrada×2
//!   + output×saída + buscas_web×US$ 0,01 (preços em `precos.json`, por prefixo do mais longo);
//! - contexto agora = input + cache_read + cache_creation da ÚLTIMA resposta do transcript principal.
//! O custo é sempre "estimado": a tabela não foi conferida contra a página de preços da Anthropic.

use crate::estado::Evento;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

const PRECOS: &str = include_str!("../precos.json");

// ------------------------------------------------------------------ preços

pub struct Tabela {
    modelos: Vec<(String, Value)>,
    rapido: Vec<(String, Value)>,
    desconhecido: Value,
    leitura: f64,
    e5m: f64,
    e1h: f64,
    busca: f64,
}

impl Tabela {
    pub fn carregar() -> Tabela {
        let v: Value = serde_json::from_str(PRECOS).expect("precos.json");
        let ordenar = |o: &Value| {
            let mut l: Vec<(String, Value)> = o.as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
            l.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
            l
        };
        Tabela {
            modelos: ordenar(&v["modelos"]),
            rapido: ordenar(&v["rapido"]),
            desconhecido: v["desconhecido"].clone(),
            leitura: v["multiplicadores"]["leituraCache"].as_f64().unwrap_or(0.1),
            e5m: v["multiplicadores"]["escritaCache5m"].as_f64().unwrap_or(1.25),
            e1h: v["multiplicadores"]["escritaCache1h"].as_f64().unwrap_or(2.0),
            busca: v["ferramentas"]["buscaWeb"].as_f64().unwrap_or(0.01),
        }
    }

    fn preco(&self, modelo: &str, rapido: bool) -> (&Value, bool) {
        if rapido {
            if let Some((_, p)) = self.rapido.iter().find(|(k, _)| modelo.starts_with(k.as_str())) {
                return (p, true);
            }
        }
        match self.modelos.iter().find(|(k, _)| modelo.starts_with(k.as_str())) {
            Some((_, p)) => (p, true),
            None => (&self.desconhecido, false),
        }
    }

    pub fn conhecido(&self, modelo: &str) -> bool {
        self.preco(modelo, false).1
    }

    pub fn custo(&self, uso: &Value, modelo: &str) -> f64 {
        let n = |p: &str| uso.pointer(p).and_then(|x| x.as_f64()).unwrap_or(0.0);
        let (p, _) = self.preco(modelo, uso.get("speed").and_then(|x| x.as_str()) == Some("fast"));
        let entrada = p["entrada"].as_f64().unwrap_or(5.0);
        let saida = p["saida"].as_f64().unwrap_or(25.0);
        let mult_leitura = p.get("leituraCache").and_then(|x| x.as_f64()).unwrap_or(self.leitura);
        let mut e1h = n("/cache_creation/ephemeral_1h_input_tokens");
        let mut e5m = n("/cache_creation/ephemeral_5m_input_tokens");
        if e1h == 0.0 && e5m == 0.0 {
            e5m = n("/cache_creation_input_tokens");
            e1h = 0.0;
        }
        let tokens = n("/input_tokens") * entrada
            + n("/cache_read_input_tokens") * entrada * mult_leitura
            + e5m * entrada * self.e5m
            + e1h * entrada * self.e1h
            + n("/output_tokens") * saida;
        tokens / 1_000_000.0 + n("/server_tool_use/web_search_requests") * self.busca
    }
}

// ------------------------------------------------------------------ leitura incremental
//
// QA A3 (06/11): a versão antiga lia o arquivo novo INTEIRO para um Vec<String> e tirava cada linha do
// começo de um Vec de 4 MB (drain), o que era quadrático por bloco: 40 s a 2 min e 923 MB na 1ª leitura.
// Agora: BufReader de 4 MB + read_until (memchr do std), cada linha vai direto para o callback sem ser
// guardada, e cada volta do motor tem um ORÇAMENTO de bytes — o resto fica para a volta seguinte, que
// vem logo (sem a pausa de 2 s), e o busy → idle das sessões continua sendo olhado a cada volta.

const BLOCO: usize = 4 * 1024 * 1024;
/// Linha maior que isto é saída de ferramenta (o que o motor procura cabe em poucos KB): é descartada.
const LINHA_MAX: usize = 16 * 1024 * 1024;
/// Bytes lidos por volta, somando todas as sessões.
pub const ORCAMENTO_VOLTA: u64 = 64 * 1024 * 1024;
/// Mínimo de cada sessão numa volta (para uma sessão gigante não parar as outras).
const ORCAMENTO_MINIMO: u64 = 8 * 1024 * 1024;

#[derive(Default)]
struct Arquivo {
    posicao: u64,
    resto: Vec<u8>,
    /// no meio de uma linha gigante: descarta até a próxima quebra de linha
    pulando: bool,
    criado: Option<SystemTime>,
}

#[derive(Debug, Default, PartialEq)]
struct Leitura {
    recomecou: bool,
    /// chegou ao fim do arquivo (nada pendente)
    em_dia: bool,
    lidos: u64,
}

fn emitir(l: &[u8], cada: &mut dyn FnMut(&str)) {
    let t = match std::str::from_utf8(l) {
        Ok(s) => std::borrow::Cow::Borrowed(s),
        Err(_) => String::from_utf8_lossy(l),
    };
    let t = t.trim();
    if !t.is_empty() {
        cada(t);
    }
}

impl Arquivo {
    /// O arquivo encolheu ou foi trocado (outro "criado")? Então recomeça do zero e devolve true.
    fn conferir_troca(&mut self, caminho: &Path) -> bool {
        let Ok(meta) = fs::metadata(caminho) else { return false };
        let criado = meta.created().ok();
        if meta.len() < self.posicao || (self.criado.is_some() && criado != self.criado) {
            *self = Arquivo::default();
            self.criado = criado;
            return true;
        }
        false
    }

    /// Lê as linhas novas (completas) até gastar o orçamento. Se o arquivo encolheu ou foi trocado, recomeça.
    fn ler(&mut self, caminho: &Path, orcamento: &mut u64, cada: &mut dyn FnMut(&str)) -> Leitura {
        let mut r = Leitura { em_dia: true, ..Default::default() };
        let Ok(meta) = fs::metadata(caminho) else { return r };
        let criado = meta.created().ok();
        if meta.len() < self.posicao || (self.criado.is_some() && criado != self.criado) {
            *self = Arquivo::default();
            r.recomecou = true;
        }
        self.criado = criado;
        if meta.len() == self.posicao {
            return r;
        }
        if *orcamento == 0 {
            r.em_dia = false;
            return r;
        }
        let Ok(mut f) = fs::File::open(caminho) else { return r };
        if f.seek(SeekFrom::Start(self.posicao)).is_err() {
            return r;
        }
        let mut leitor = BufReader::with_capacity(BLOCO.min((meta.len() - self.posicao) as usize + 1).max(8 * 1024), f);
        let mut linha = std::mem::take(&mut self.resto);
        while *orcamento > 0 {
            let cabe = (LINHA_MAX as u64).saturating_sub(linha.len() as u64).max(1);
            let n = match (&mut leitor).take(cabe).read_until(b'\n', &mut linha) {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 {
                break; // fim do arquivo
            }
            self.posicao += n as u64;
            *orcamento = orcamento.saturating_sub(n as u64);
            r.lidos += n as u64;
            if linha.last() == Some(&b'\n') {
                if !self.pulando {
                    emitir(&linha, cada);
                }
                self.pulando = false;
                linha.clear();
            } else if linha.len() >= LINHA_MAX {
                self.pulando = true;
                linha.clear();
            }
            // senão: o arquivo acabou no meio de uma linha; ela fica guardada até completar
        }
        self.resto = if self.pulando || linha.is_empty() { Vec::new() } else { linha };
        r.em_dia = self.posicao >= meta.len();
        r
    }

    /// Para os testes: lê tudo de uma vez e devolve as linhas.
    #[cfg(test)]
    fn linhas_novas(&mut self, caminho: &Path) -> (Vec<String>, bool) {
        let mut v = vec![];
        let mut orc = u64::MAX;
        let r = self.ler(caminho, &mut orc, &mut |l| v.push(l.to_string()));
        (v, r.recomecou)
    }
}

#[derive(Clone, Default)]
struct Lancamento {
    arquivo: PathBuf,
    modelo: String,
    entrada: u64,
    saida: u64,
    leitura: u64,
    escrita: u64,
    custo: f64,
}

#[derive(Clone)]
struct Agente {
    chamada: String,
    nome: String,
    tipo: String,
    /// id do agente-pai (arquivo agent-<id>) ou None = a conversa principal
    pai: Option<String>,
    inicio: String,
    terminou: bool,
}

pub struct Sessao {
    pub id: String,
    pub transcript: PathBuf,
    arquivos: HashMap<PathBuf, Arquivo>,
    contabilizado: HashMap<String, Lancamento>,
    ciclos: u32,
    rotulo_custom: String,
    rotulo_ai: String,
    rotulo_pedido: String,
    skills: HashMap<String, u32>,
    agentes: Vec<Agente>,
    /// chamada (tool_use id) → id do arquivo de subagente
    ligacoes: HashMap<String, String>,
    contexto: u64,
    ultimo_modelo: String,
    desconhecidos: HashSet<String>,
    /// (timestamp ISO, ttl em minutos, suposto)
    cache: Option<(String, u32, bool)>,
    em_dia: bool,
}

impl Sessao {
    pub fn nova(id: &str, transcript: PathBuf) -> Sessao {
        Sessao {
            id: id.into(),
            transcript,
            arquivos: HashMap::new(),
            contabilizado: HashMap::new(),
            ciclos: 0,
            rotulo_custom: String::new(),
            rotulo_ai: String::new(),
            rotulo_pedido: String::new(),
            skills: HashMap::new(),
            agentes: vec![],
            ligacoes: HashMap::new(),
            contexto: 0,
            ultimo_modelo: String::new(),
            desconhecidos: HashSet::new(),
            cache: None,
            em_dia: false,
        }
    }

    pub fn nome(&self) -> String {
        if !self.rotulo_custom.is_empty() {
            return self.rotulo_custom.clone();
        }
        if !self.rotulo_ai.is_empty() {
            return self.rotulo_ai.clone();
        }
        if !self.rotulo_pedido.is_empty() {
            return crate::estado::cortar(&self.rotulo_pedido, 61);
        }
        self.id.chars().take(8).collect()
    }

    fn pasta_subagentes(&self) -> PathBuf {
        self.transcript.with_extension("").join("subagents")
    }

    /// Lê o que há de novo (tudo, sem orçamento). Devolve true se algo mudou.
    #[cfg(test)]
    pub fn atualizar(&mut self, t: &Tabela) -> bool {
        let mut orc = u64::MAX;
        self.atualizar_com(t, &mut orc)
    }

    /// Já leu cada arquivo até o fim pelo menos uma vez? Antes disso os números estariam pela metade.
    pub fn em_dia(&self) -> bool {
        self.em_dia
    }

    /// Lê o que há de novo, até gastar o orçamento. Devolve true se algo mudou.
    pub fn atualizar_com(&mut self, t: &Tabela, orcamento: &mut u64) -> bool {
        self.ciclos += 1;
        // o principal e os subagentes já conhecidos a cada volta; arquivo novo de subagente a cada 5 voltas
        let mut caminhos = vec![self.transcript.clone()];
        caminhos.extend(self.arquivos.keys().filter(|k| **k != self.transcript).cloned());
        if self.ciclos == 1 || self.ciclos % 5 == 0 {
            if let Ok(l) = fs::read_dir(self.pasta_subagentes()) {
                for e in l.flatten() {
                    let p = e.path();
                    if p.extension().map(|x| x == "jsonl").unwrap_or(false) && !caminhos.contains(&p) {
                        caminhos.push(p);
                    }
                }
            }
        }
        let mut mudou = false;
        let mut tudo_em_dia = true;
        for c in caminhos {
            let principal = c == self.transcript;
            let mut a = self.arquivos.remove(&c).unwrap_or_default();
            if a.conferir_troca(&c) {
                // o arquivo foi trocado: o que veio dele sai da conta e é lido de novo
                self.contabilizado.retain(|_, l| l.arquivo != c);
                mudou = true;
            }
            let dono = if principal { None } else { c.file_stem().map(|s| s.to_string_lossy().trim_start_matches("agent-").to_string()) };
            let mut linhas_mudaram = false;
            let r = a.ler(&c, orcamento, &mut |l| {
                if self.linha(l, &c, principal, &dono, t) {
                    linhas_mudaram = true;
                }
            });
            self.arquivos.insert(c.clone(), a);
            if r.recomecou {
                mudou = true;
            }
            if linhas_mudaram {
                mudou = true;
            }
            if !r.em_dia {
                tudo_em_dia = false;
            }
        }
        if tudo_em_dia && !self.em_dia {
            self.em_dia = true;
            mudou = true;
        }
        if mudou {
            self.ler_metas();
        }
        mudou
    }

    fn ler_metas(&mut self) {
        // .meta.json dos subagentes: nome e tipo mesmo em conversa retomada ou de outro programa
        let Ok(l) = fs::read_dir(self.pasta_subagentes()) else { return };
        for e in l.flatten() {
            let p = e.path();
            let nome = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if !nome.ends_with(".meta.json") {
                continue;
            }
            let id = nome.trim_start_matches("agent-").trim_end_matches(".meta.json").to_string();
            let Ok(v) = fs::read_to_string(&p).map(|t| serde_json::from_str::<Value>(&t).unwrap_or(Value::Null)) else { continue };
            let chamada = v.get("toolUseId").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if chamada.is_empty() {
                continue;
            }
            self.ligacoes.entry(chamada.clone()).or_insert(id);
            if !self.agentes.iter().any(|a| a.chamada == chamada) {
                self.agentes.push(Agente {
                    chamada,
                    nome: v.get("description").and_then(|x| x.as_str()).unwrap_or("subagente").to_string(),
                    tipo: v.get("agentType").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    pai: None,
                    inicio: String::new(),
                    terminou: true,
                });
            }
        }
    }

    fn linha(&mut self, linha: &str, arquivo: &Path, principal: bool, dono: &Option<String>, t: &Tabela) -> bool {
        // filtro barato antes do JSON: transcript de subagente passa de 1 GB, e quase tudo é saída de ferramenta
        let interessa = linha.contains("\"usage\"")
            || linha.contains("agentId:")
            || (principal && (linha.contains("-title\"") || linha.contains("\"last-prompt\"") || linha.contains("<command-name>/")));
        if !interessa {
            return false;
        }
        let Ok(o) = serde_json::from_str::<Value>(linha) else { return false };
        let tipo = o.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let s = |k: &str| o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        match tipo {
            "custom-title" if principal => {
                self.rotulo_custom = s("customTitle");
                return true;
            }
            "ai-title" if principal => {
                self.rotulo_ai = s("aiTitle");
                return true;
            }
            "last-prompt" if principal => {
                self.rotulo_pedido = s("lastPrompt");
                return true;
            }
            "user" => {
                let mut mudou = false;
                if let Some(blocos) = o.pointer("/message/content").and_then(|x| x.as_array()) {
                    for b in blocos {
                        if b.get("type").and_then(|x| x.as_str()) != Some("tool_result") {
                            continue;
                        }
                        let chamada = b.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let texto = match b.get("content") {
                            Some(Value::String(t)) => t.clone(),
                            Some(Value::Array(a)) => a.iter().filter_map(|p| p.get("text").and_then(|x| x.as_str())).collect::<Vec<_>>().join(" "),
                            _ => String::new(),
                        };
                        if let Some(i) = texto.find("agentId:") {
                            let id: String = texto[i + 8..].trim_start().chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                            if !id.is_empty() {
                                self.ligacoes.insert(chamada.clone(), id);
                            }
                        }
                        if let Some(a) = self.agentes.iter_mut().find(|a| a.chamada == chamada) {
                            a.terminou = true;
                            mudou = true;
                        }
                    }
                } else if principal {
                    // skill chamada pela barra: <command-name>/nome</command-name>
                    if let Some(t) = o.pointer("/message/content").and_then(|x| x.as_str()) {
                        if let (Some(a), Some(b)) = (t.find("<command-name>/"), t.find("</command-name>")) {
                            if b > a + 15 {
                                let nome = t[a + 15..b].trim().to_string();
                                *self.skills.entry(nome).or_insert(0) += 1;
                                mudou = true;
                            }
                        }
                    }
                }
                return mudou;
            }
            "assistant" => {}
            _ => return false,
        }
        let Some(msg) = o.get("message") else { return false };
        let (Some(id), Some(uso)) = (msg.get("id").and_then(|x| x.as_str()), msg.get("usage")) else { return false };
        let modelo = msg.get("model").and_then(|x| x.as_str()).unwrap_or("desconhecido").to_string();
        // ações (antes da deduplicação: a linha com o tool_use costuma ser uma das repetidas)
        if let Some(blocos) = msg.get("content").and_then(|x| x.as_array()) {
            for b in blocos {
                if b.get("type").and_then(|x| x.as_str()) != Some("tool_use") {
                    continue;
                }
                let nome = b.get("name").and_then(|x| x.as_str()).unwrap_or("");
                let ent = b.get("input").cloned().unwrap_or(Value::Null);
                if nome == "Skill" && principal {
                    if let Some(sk) = ent.get("skill").and_then(|x| x.as_str()) {
                        *self.skills.entry(sk.to_string()).or_insert(0) += 1;
                    }
                } else if nome == "Agent" || nome == "Task" {
                    let chamada = b.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    if !chamada.is_empty() && !self.agentes.iter().any(|a| a.chamada == chamada) {
                        self.agentes.push(Agente {
                            chamada,
                            nome: ent.get("description").and_then(|x| x.as_str()).or(ent.get("subagent_type").and_then(|x| x.as_str())).unwrap_or("subagente").to_string(),
                            tipo: ent.get("subagent_type").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            pai: dono.clone(),
                            inicio: s("timestamp"),
                            terminou: false,
                        });
                    }
                }
            }
        }
        if modelo == "<synthetic>" {
            return false;
        }
        if principal {
            let n = |p: &str| uso.pointer(p).and_then(|x| x.as_u64()).unwrap_or(0);
            let ctx = n("/input_tokens") + n("/cache_read_input_tokens") + n("/cache_creation_input_tokens");
            if ctx > 0 {
                self.contexto = ctx;
            }
            let (h1, m5) = (n("/cache_creation/ephemeral_1h_input_tokens"), n("/cache_creation/ephemeral_5m_input_tokens"));
            let (ttl, suposto) = if m5 > 0 { (5, false) } else if h1 > 0 { (60, false) } else { (60, true) };
            self.cache = Some((s("timestamp"), ttl, suposto));
        }
        self.contabilizar(id, uso, &modelo, arquivo, t)
    }

    fn contabilizar(&mut self, id: &str, uso: &Value, modelo: &str, arquivo: &Path, t: &Tabela) -> bool {
        let n = |p: &str| uso.pointer(p).and_then(|x| x.as_u64()).unwrap_or(0);
        let saida = n("/output_tokens");
        if let Some(ant) = self.contabilizado.get(id) {
            if saida <= ant.saida {
                return false;
            }
        }
        self.contabilizado.insert(
            id.to_string(),
            Lancamento {
                arquivo: arquivo.to_path_buf(),
                modelo: modelo.to_string(),
                entrada: n("/input_tokens"),
                saida,
                leitura: n("/cache_read_input_tokens"),
                escrita: n("/cache_creation_input_tokens"),
                custo: t.custo(uso, modelo),
            },
        );
        self.ultimo_modelo = modelo.to_string();
        if !t.conhecido(modelo) {
            self.desconhecidos.insert(modelo.to_string());
        }
        true
    }

    pub fn resumo(&self) -> Value {
        let (mut ent, mut sai, mut lei, mut esc, mut custo) = (0u64, 0u64, 0u64, 0u64, 0f64);
        let mut por_modelo: HashMap<String, (u64, f64, u32)> = HashMap::new();
        let mut por_arquivo: HashMap<PathBuf, (u64, f64, u32)> = HashMap::new();
        for l in self.contabilizado.values() {
            ent += l.entrada;
            sai += l.saida;
            lei += l.leitura;
            esc += l.escrita;
            custo += l.custo;
            let tk = l.entrada + l.saida + l.leitura + l.escrita;
            let m = por_modelo.entry(l.modelo.clone()).or_default();
            m.0 += tk;
            m.1 += l.custo;
            m.2 += 1;
            let a = por_arquivo.entry(l.arquivo.clone()).or_default();
            a.0 += l.entrada + l.escrita + l.saida;
            a.1 += l.custo;
            a.2 += 1;
        }
        let pasta = self.pasta_subagentes();
        let gasto_do = |id: &str| por_arquivo.get(&pasta.join(format!("agent-{id}.jsonl"))).cloned().unwrap_or_default();
        let mut casados = HashSet::new();
        let mut agentes: Vec<Value> = self
            .agentes
            .iter()
            .map(|a| {
                let id = self.ligacoes.get(&a.chamada).cloned();
                if let Some(i) = &id {
                    casados.insert(i.clone());
                }
                let g = id.as_deref().map(gasto_do).unwrap_or_default();
                json!({ "chamada": a.chamada, "id": id, "nome": a.nome, "tipo": a.tipo, "pai": a.pai, "inicio": a.inicio,
                        "estado": if a.terminou { "terminou" } else { "rodando" }, "tokens": g.0, "custo": g.1, "respostas": g.2 })
            })
            .collect();
        for (arq, g) in &por_arquivo {
            if arq == &self.transcript {
                continue;
            }
            let id = arq.file_stem().map(|s| s.to_string_lossy().trim_start_matches("agent-").to_string()).unwrap_or_default();
            if !casados.contains(&id) && g.2 > 0 {
                agentes.push(json!({ "chamada": null, "id": id, "nome": format!("subagente {}", id.chars().take(6).collect::<String>()), "tipo": "", "pai": null, "inicio": "", "estado": "terminou", "tokens": g.0, "custo": g.1, "respostas": g.2 }));
            }
        }
        let mut skills: Vec<(String, u32)> = self.skills.iter().map(|(k, v)| (k.clone(), *v)).collect();
        skills.sort_by(|a, b| b.1.cmp(&a.1));
        let mut modelos: Vec<Value> = por_modelo.iter().map(|(k, v)| json!({ "modelo": k, "tokens": v.0, "custo": v.1, "respostas": v.2, "precoConhecido": !self.desconhecidos.contains(k) })).collect();
        modelos.sort_by(|a, b| b["custo"].as_f64().partial_cmp(&a["custo"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
        json!({
            "sessao": self.id,
            "nome": self.nome(),
            "modelo": self.ultimo_modelo,
            "contexto": self.contexto,
            "totalProcessado": ent + sai + lei + esc,
            "materialNovo": ent + esc + sai,
            "entrada": ent, "saida": sai, "leituraCache": lei, "escritaCache": esc,
            "custo": custo,
            "custoEstimado": true,
            "respostas": self.contabilizado.len(),
            "porModelo": modelos,
            "agentes": agentes,
            "skills": skills.iter().map(|(n, v)| json!({ "nome": n, "vezes": v })).collect::<Vec<_>>(),
            "modelosDesconhecidos": self.desconhecidos.iter().collect::<Vec<_>>(),
            "cache": self.cache.as_ref().map(|(ts, ttl, sup)| json!({ "desde": ts, "ttlMinutos": ttl, "suposto": sup })),
        })
    }
}

// ------------------------------------------------------------------ pastas do Claude Code

pub fn pasta_claude() -> PathBuf {
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(".claude")
}

/// Nome da pasta em `projects/` para um cwd (tudo que não é letra ou número vira "-").
pub fn pasta_do_projeto(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn achar_transcript(sessao: &str, cwd: &str) -> Option<PathBuf> {
    let projetos = pasta_claude().join("projects");
    let alvo = pasta_do_projeto(cwd).to_lowercase();
    if let Ok(l) = fs::read_dir(&projetos) {
        let dirs: Vec<PathBuf> = l.flatten().map(|e| e.path()).collect();
        // primeiro a pasta do cwd; se não achar, procura em todas
        for d in dirs.iter().filter(|d| d.file_name().map(|n| n.to_string_lossy().to_lowercase() == alvo).unwrap_or(false)).chain(dirs.iter()) {
            let p = d.join(format!("{sessao}.jsonl"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(windows)]
pub fn pid_vivo(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    if pid == 0 {
        return false;
    }
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut codigo: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut codigo) != 0;
        CloseHandle(h);
        ok && codigo == STILL_ACTIVE as u32
    }
}

#[cfg(not(windows))]
pub fn pid_vivo(_pid: u32) -> bool {
    true
}

#[derive(Clone, Debug)]
pub struct Viva {
    pub pid: u32,
    pub sessao: String,
    pub cwd: String,
    pub nome: String,
    pub status: String,
    pub entrypoint: String,
    pub atualizada: u64,
}

pub fn sessoes_vivas() -> Vec<Viva> {
    let mut v = vec![];
    let Ok(l) = fs::read_dir(pasta_claude().join("sessions")) else { return v };
    for e in l.flatten() {
        let p = e.path();
        if p.extension().map(|x| x != "json").unwrap_or(true) {
            continue;
        }
        let Ok(o) = fs::read_to_string(&p).map(|t| serde_json::from_str::<Value>(&t).unwrap_or(Value::Null)) else { continue };
        let pid = o.get("pid").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        if !pid_vivo(pid) {
            continue;
        }
        let s = |k: &str| o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let sessao = s("sessionId");
        if sessao.is_empty() {
            continue;
        }
        v.push(Viva {
            pid,
            sessao,
            cwd: s("cwd"),
            nome: s("name"),
            status: s("status"),
            entrypoint: s("entrypoint"),
            atualizada: o.get("updatedAt").and_then(|x| x.as_u64()).unwrap_or(0),
        });
    }
    v
}

// ------------------------------------------------------------------ o motor

pub type AoResumo = Arc<dyn Fn(Value) + Send + Sync>;
pub type AoEstado = Arc<dyn Fn(Evento) + Send + Sync>;

pub struct Motor {
    tabela: Tabela,
    sessoes: Mutex<HashMap<String, Sessao>>,
    vivas: Mutex<Vec<Viva>>,
    status_antes: Mutex<HashMap<u32, String>>,
    ultimo_resumo: Mutex<Value>,
    limites: Mutex<Value>,
    cutucada: Mutex<bool>,
}

impl Motor {
    pub fn novo() -> Arc<Motor> {
        Arc::new(Motor {
            tabela: Tabela::carregar(),
            sessoes: Mutex::new(HashMap::new()),
            vivas: Mutex::new(vec![]),
            status_antes: Mutex::new(HashMap::new()),
            ultimo_resumo: Mutex::new(json!({ "sessoes": [], "total": { "custo": 0.0, "tokens": 0 }, "precosConferidos": false })),
            limites: Mutex::new(json!({ "disponivel": false })),
            cutucada: Mutex::new(false),
        })
    }

    pub fn cutucar(&self) {
        *self.cutucada.lock().unwrap() = true;
    }

    pub fn resumo(&self) -> Value {
        self.ultimo_resumo.lock().unwrap().clone()
    }

    pub fn limites(&self) -> Value {
        self.limites.lock().unwrap().clone()
    }

    pub fn iniciar(self: &Arc<Self>, ao_resumo: AoResumo, ao_estado: AoEstado) {
        let eu = self.clone();
        std::thread::Builder::new()
            .name("ponte-tokens".into())
            .spawn(move || {
                let mut ultimo_limite = Instant::now() - Duration::from_secs(600);
                loop {
                    let (mudou, pendente) = eu.volta(&ao_estado);
                    if mudou {
                        ao_resumo(eu.resumo());
                    }
                    if ultimo_limite.elapsed() > Duration::from_secs(60) {
                        ultimo_limite = Instant::now();
                        eu.ler_limites();
                    }
                    if pendente {
                        // ainda há transcript para ler: a próxima volta vem logo (com uma folga para a CPU)
                        std::thread::sleep(Duration::from_millis(40));
                        continue;
                    }
                    for _ in 0..20 {
                        std::thread::sleep(Duration::from_millis(100));
                        if std::mem::take(&mut *eu.cutucada.lock().unwrap()) {
                            break;
                        }
                    }
                }
            })
            .ok();
    }

    /// Uma volta: sessões vivas, busy → idle (2ª fonte de "pronto") e tokens de cada uma.
    /// Devolve (mudou, pendente): pendente = algum transcript ficou para a próxima volta (orçamento).
    fn volta(&self, ao_estado: &AoEstado) -> (bool, bool) {
        let vivas = sessoes_vivas();
        let mut mudou = false;
        {
            let mut antes = self.status_antes.lock().unwrap();
            for v in &vivas {
                let anterior = antes.insert(v.pid, v.status.clone());
                if anterior.as_deref() == Some("busy") && v.status == "idle" {
                    let projeto = Path::new(&v.cwd).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    ao_estado(Evento { tipo: "pronto".into(), sessao: v.sessao.clone(), projeto, titulo: v.nome.clone(), origem: "sessoes".into(), ..Default::default() });
                }
                if anterior.as_deref() != Some(v.status.as_str()) {
                    mudou = true;
                }
            }
            let pids: HashSet<u32> = vivas.iter().map(|v| v.pid).collect();
            let n = antes.len();
            antes.retain(|p, _| pids.contains(p));
            if antes.len() != n {
                mudou = true;
            }
        }
        let mut sessoes = self.sessoes.lock().unwrap();
        let ids: HashSet<String> = vivas.iter().map(|v| v.sessao.clone()).collect();
        let n = sessoes.len();
        sessoes.retain(|k, _| ids.contains(k));
        if sessoes.len() != n {
            mudou = true;
        }
        // orçamento da volta dividido entre as sessões (cada uma com um mínimo)
        let por_sessao = (ORCAMENTO_VOLTA / vivas.len().max(1) as u64).max(ORCAMENTO_MINIMO);
        let mut pendente = false;
        for v in &vivas {
            if !sessoes.contains_key(&v.sessao) {
                if let Some(t) = achar_transcript(&v.sessao, &v.cwd) {
                    sessoes.insert(v.sessao.clone(), Sessao::nova(&v.sessao, t));
                }
            }
            if let Some(s) = sessoes.get_mut(&v.sessao) {
                let mut orc = por_sessao;
                if s.atualizar_com(&self.tabela, &mut orc) {
                    mudou = true;
                }
                if orc == 0 || !s.em_dia() {
                    pendente = true;
                }
            }
        }
        if mudou {
            let mut lista = vec![];
            let (mut custo, mut tokens) = (0f64, 0u64);
            for v in &vivas {
                // sessão ainda na 1ª leitura: sem números pela metade (a tela mostra "lendo")
                let mut r = match sessoes.get(&v.sessao) {
                    Some(s) if s.em_dia() => s.resumo(),
                    Some(s) => json!({ "sessao": v.sessao, "nome": s.nome(), "medido": false, "lendo": true }),
                    None => json!({ "sessao": v.sessao, "nome": v.nome, "medido": false }),
                };
                if !v.nome.is_empty() && r.get("nome").and_then(|x| x.as_str()).map(|n| n.len() <= 8).unwrap_or(true) {
                    r["nome"] = json!(v.nome);
                }
                r["pid"] = json!(v.pid);
                r["cwd"] = json!(v.cwd);
                r["status"] = json!(v.status);
                r["origem"] = json!(v.entrypoint);
                r["atualizadaEm"] = json!(v.atualizada);
                custo += r["custo"].as_f64().unwrap_or(0.0);
                tokens += r["totalProcessado"].as_u64().unwrap_or(0);
                lista.push(r);
            }
            *self.ultimo_resumo.lock().unwrap() = json!({ "sessoes": lista, "total": { "custo": custo, "tokens": tokens, "custoEstimado": true }, "precosConferidos": false });
        }
        *self.vivas.lock().unwrap() = vivas;
        (mudou, pendente)
    }

    /// Limites de 5 h e 7 dias, de `~/.claude.json` (sem rede). Falha nunca apaga o anterior.
    pub fn ler_limites(&self) {
        let caminho = std::env::var("CLAUDE_CONFIG_DIR").map(|d| PathBuf::from(d).join(".claude.json")).unwrap_or_else(|_| PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(".claude.json"));
        let Ok(t) = fs::read_to_string(&caminho) else { return };
        let Ok(v) = serde_json::from_str::<Value>(&t) else { return };
        let Some(u) = v.pointer("/cachedUsageUtilization/utilization") else { return };
        let janela = |k: &str| {
            u.get(k).filter(|x| x.is_object()).map(|x| json!({ "porcento": x.get("utilization").and_then(|y| y.as_f64()), "zeraEm": x.get("resets_at").and_then(|y| y.as_str()) }))
        };
        let novo = json!({
            "disponivel": true,
            "cincoHoras": janela("five_hour"),
            "seteDias": janela("seven_day"),
            "lidoEm": v.pointer("/cachedUsageUtilization/fetchedAtMs").and_then(|x| x.as_u64()),
            "fonte": "~/.claude.json (cache do Claude Code, pode estar atrasado)",
        });
        if novo["cincoHoras"].is_null() && novo["seteDias"].is_null() {
            return;
        }
        *self.limites.lock().unwrap() = novo;
    }
}

// ------------------------------------------------------------------ conversas antigas

/// Conversas de outros dias de uma pasta (para retomar).
pub fn conversas_antigas(pasta: &Path, limite: usize) -> Value {
    let cwd = crate::arquivos::exibir(pasta);
    let alvo = pasta_do_projeto(&cwd).to_lowercase();
    let mut itens = vec![];
    if let Ok(l) = fs::read_dir(pasta_claude().join("projects")) {
        for d in l.flatten().filter(|d| d.file_name().to_string_lossy().to_lowercase() == alvo) {
            if let Ok(arqs) = fs::read_dir(d.path()) {
                for a in arqs.flatten() {
                    let p = a.path();
                    if p.extension().map(|x| x == "jsonl").unwrap_or(false) {
                        let meta = a.metadata().ok();
                        let quando = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
                        itens.push((quando, p, meta.map(|m| m.len()).unwrap_or(0)));
                    }
                }
            }
        }
    }
    itens.sort_by(|a, b| b.0.cmp(&a.0));
    let lista: Vec<Value> = itens
        .into_iter()
        .take(limite)
        .map(|(quando, p, tam)| {
            let id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            json!({ "sessao": id, "nome": nome_pela_cauda(&p).unwrap_or_else(|| id.chars().take(8).collect()), "atualizadaEm": quando, "bytes": tam, "pasta": cwd })
        })
        .collect();
    json!({ "pasta": cwd, "conversas": lista })
}

/// Nome da conversa lendo só o fim do arquivo (custom-title > ai-title > last-prompt).
fn nome_pela_cauda(p: &Path) -> Option<String> {
    let mut f = fs::File::open(p).ok()?;
    let tam = f.metadata().ok()?.len();
    let ini = tam.saturating_sub(256 * 1024);
    f.seek(SeekFrom::Start(ini)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let texto = String::from_utf8_lossy(&buf);
    let (mut c, mut a, mut l) = (None, None, None);
    for linha in texto.lines() {
        if !linha.contains("-title") && !linha.contains("last-prompt") {
            continue;
        }
        let Ok(o) = serde_json::from_str::<Value>(linha) else { continue };
        match o.get("type").and_then(|x| x.as_str()) {
            Some("custom-title") => c = o.get("customTitle").and_then(|x| x.as_str()).map(String::from),
            Some("ai-title") => a = o.get("aiTitle").and_then(|x| x.as_str()).map(String::from),
            Some("last-prompt") => l = o.get("lastPrompt").and_then(|x| x.as_str()).map(|s| crate::estado::cortar(s, 61)),
            _ => {}
        }
    }
    c.or(a).or(l)
}

// ------------------------------------------------------------------ conta e MCP (claude CLI, sem shell)


pub fn rodar_claude_publico(args: &[&str], cwd: Option<&Path>, limite: Duration) -> Result<String, String> {
    rodar_claude(args, cwd, limite)
}

fn rodar_claude(args: &[&str], cwd: Option<&Path>, limite: Duration) -> Result<String, String> {
    let exe = crate::job::claude_exe().ok_or("não achei o claude.exe")?;
    let mut cmd = Command::new(exe);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).env_remove("CLAUDECODE");
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut f = cmd.spawn().map_err(|e| e.to_string())?;
    crate::job::amarrar(f.id());
    let mut saida = f.stdout.take().unwrap();
    let leitor = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = saida.read_to_string(&mut s);
        s
    });
    let fim = Instant::now() + limite;
    loop {
        if let Ok(Some(_)) = f.try_wait() {
            break;
        }
        if Instant::now() > fim {
            // QA B8: derruba a árvore inteira (o `claude mcp list` abre os servidores MCP como netos)
            let mut tk = Command::new(r"C:\Windows\System32\taskkill.exe");
            tk.args(["/PID", &f.id().to_string(), "/T", "/F"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                tk.creation_flags(0x0800_0000);
            }
            let _ = tk.status();
            let _ = f.kill();
            return Err("o claude demorou demais".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(leitor.join().unwrap_or_default())
}

/// `claude auth status --json`, só com o que a tela mostra.
pub fn conta() -> Value {
    match rodar_claude(&["auth", "status", "--json"], None, Duration::from_secs(20)) {
        Ok(t) => match serde_json::from_str::<Value>(t.trim()) {
            Ok(v) => json!({
                "ok": true,
                "logado": v.get("loggedIn").and_then(|x| x.as_bool()),
                "metodo": v.get("authMethod"),
                "email": v.get("email"),
                "organizacao": v.get("orgName"),
                "plano": v.get("subscriptionType"),
            }),
            Err(_) => json!({ "ok": false, "erro": "resposta inesperada do claude auth status" }),
        },
        Err(e) => json!({ "ok": false, "erro": e }),
    }
}

/// `claude mcp list`, traduzido. O formato é texto e pode mudar; o bruto vai junto.
pub fn conexoes_mcp(cwd: &Path) -> Value {
    match rodar_claude(&["mcp", "list"], Some(cwd), Duration::from_secs(90)) {
        Ok(t) => json!({ "ok": true, "conexoes": traduzir_mcp(&t), "bruto": crate::estado::cortar(&t, 4000) }),
        Err(e) => json!({ "ok": false, "erro": e }),
    }
}

pub fn traduzir_mcp(texto: &str) -> Vec<Value> {
    texto
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (nome, resto) = l.split_once(": ")?;
            if nome.contains(' ') && !nome.starts_with("claude.ai") && !nome.starts_with("plugin:") {
                return None;
            }
            let estado = if resto.contains('✓') || resto.contains("Connected") {
                "conectado"
            } else if resto.to_lowercase().contains("auth") {
                "precisa de login"
            } else if resto.contains('✗') || resto.contains("Failed") {
                "falhou"
            } else {
                return None;
            };
            Some(json!({ "nome": nome, "estado": estado, "detalhe": resto.rsplit(" - ").next().unwrap_or("").trim() }))
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn custo_e_prefixo() {
        let t = Tabela::carregar();
        let uso = json!({ "input_tokens": 1_000_000, "output_tokens": 1_000_000 });
        assert!((t.custo(&uso, "claude-haiku-4-5-20251001") - 6.0).abs() < 1e-9);
        let cache = json!({ "cache_read_input_tokens": 1_000_000, "cache_creation_input_tokens": 1_000_000 });
        // opus 5: leitura 5×0,1 + escrita 5m 5×1,25 = 0,5 + 6,25
        assert!((t.custo(&cache, "claude-opus-5") - 6.75).abs() < 1e-9);
        let fast = json!({ "output_tokens": 1_000_000, "speed": "fast" });
        assert!((t.custo(&fast, "claude-opus-4-8") - 50.0).abs() < 1e-9);
        assert!(!t.conhecido("claude-haiku-5-5"));
        assert!(t.conhecido("claude-opus-4-8-20260101"));
    }

    #[test]
    fn deduplica_pela_maior_saida_e_le_so_o_novo() {
        let d = std::env::temp_dir().join(format!("ponte-tok-{}", crate::config::aleatorio_hex(4)));
        fs::create_dir_all(&d).unwrap();
        let arq = d.join("s1.jsonl");
        let l = |id: &str, out: u64| format!("{{\"type\":\"assistant\",\"timestamp\":\"2026-11-05T10:00:00Z\",\"message\":{{\"id\":\"{id}\",\"model\":\"claude-haiku-4-5\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{out},\"cache_read_input_tokens\":100,\"cache_creation\":{{\"ephemeral_5m_input_tokens\":3}},\"cache_creation_input_tokens\":3}}}}}}\n");
        fs::write(&arq, format!("{}{}{{\"type\":\"custom-title\",\"customTitle\":\"Minha conversa\"}}\n", l("m1", 5), l("m1", 50))).unwrap();
        let t = Tabela::carregar();
        let mut s = Sessao::nova("s1", arq.clone());
        assert!(s.atualizar(&t));
        let r = s.resumo();
        assert_eq!(r["saida"], 50);
        assert_eq!(r["respostas"], 1);
        assert_eq!(r["nome"], "Minha conversa");
        assert_eq!(r["contexto"], 113);
        assert_eq!(r["cache"]["ttlMinutos"], 5);
        assert!(!s.atualizar(&t)); // nada novo
        // linha incompleta fica guardada até completar
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&arq).unwrap();
        let nova = l("m2", 7);
        f.write_all(&nova.as_bytes()[..20]).unwrap();
        f.flush().unwrap();
        assert!(!s.atualizar(&t));
        f.write_all(&nova.as_bytes()[20..]).unwrap();
        f.flush().unwrap();
        assert!(s.atualizar(&t));
        assert_eq!(s.resumo()["saida"], 57);
        fs::remove_dir_all(&d).ok();
    }

    /// QA A3: em pedaços (orçamento pequeno) dá o mesmo resultado que tudo de uma vez, a linha
    /// gigante é pulada sem guardar memória, a truncada no fim não vira resposta, e é linear.
    #[test]
    fn leitura_incremental_por_orcamento() {
        use std::io::Write;
        let d = std::env::temp_dir().join(format!("ponte-orc-{}", crate::config::aleatorio_hex(4)));
        fs::create_dir_all(&d).unwrap();
        let arq = d.join("s3.jsonl");
        let mut f = fs::File::create(&arq).unwrap();
        let curta = format!("{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"x\",\"content\":\"{}\"}}]}}}}\n", "r".repeat(180));
        let mut respostas = 0u64;
        for i in 0..60_000u64 {
            if i % 200 == 0 {
                respostas += 1;
                writeln!(f, "{{\"type\":\"assistant\",\"message\":{{\"id\":\"m{i}\",\"model\":\"claude-haiku-4-5\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":2}}}}}}").unwrap();
            } else if i == 30_000 {
                // linha gigante (saída de ferramenta) com 20 MB, maior que LINHA_MAX
                f.write_all(b"{\"type\":\"user\",\"x\":\"").unwrap();
                f.write_all(&vec![b'z'; 20 * 1024 * 1024]).unwrap();
                f.write_all(b"\"}\n").unwrap();
            } else {
                f.write_all(curta.as_bytes()).unwrap();
            }
        }
        f.write_all(b"{\"type\":\"assistant\",\"message\":{\"id\":\"truncada\"").unwrap();
        drop(f);
        let t = Tabela::carregar();
        let t0 = Instant::now();
        let mut inteira = Sessao::nova("s3", arq.clone());
        inteira.atualizar(&t);
        let tempo = t0.elapsed();
        assert!(inteira.em_dia());
        assert_eq!(inteira.resumo()["respostas"], respostas);
        let mut aos_poucos = Sessao::nova("s3", arq.clone());
        let mut voltas = 0;
        while !aos_poucos.em_dia() {
            let mut orc = 512 * 1024;
            aos_poucos.atualizar_com(&t, &mut orc);
            voltas += 1;
            assert!(voltas < 500, "não terminou");
        }
        assert!(voltas > 10, "o orçamento não dividiu a leitura ({voltas} voltas)");
        assert_eq!(aos_poucos.resumo()["respostas"], respostas);
        assert_eq!(aos_poucos.resumo()["saida"], inteira.resumo()["saida"]);
        // o resto guardado é só a linha truncada do fim, não a linha gigante
        let a = aos_poucos.arquivos.get(&arq).unwrap();
        assert!(a.resto.len() < 100, "resto de {} bytes", a.resto.len());
        // ~32 MB: a versão antiga levava dezenas de segundos; esta, bem menos (mesmo no debug)
        assert!(tempo < Duration::from_secs(10), "{tempo:?}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn subagente_com_nome() {
        let d = std::env::temp_dir().join(format!("ponte-sub-{}", crate::config::aleatorio_hex(4)));
        let sub = d.join("s2").join("subagents");
        fs::create_dir_all(&sub).unwrap();
        let arq = d.join("s2.jsonl");
        fs::write(&arq, concat!(
            "{\"type\":\"assistant\",\"message\":{\"id\":\"a1\",\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"content\":[{\"type\":\"tool_use\",\"id\":\"tu1\",\"name\":\"Agent\",\"input\":{\"description\":\"Lia monta a tela\",\"subagent_type\":\"design-ui\"}}]}}\n",
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"tu1\",\"content\":\"feito. agentId: abc123\"}]}}\n"
        )).unwrap();
        fs::write(sub.join("agent-abc123.jsonl"), "{\"type\":\"assistant\",\"message\":{\"id\":\"x1\",\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":1000,\"output_tokens\":2000}}}\n").unwrap();
        let t = Tabela::carregar();
        let mut s = Sessao::nova("s2", arq);
        s.atualizar(&t);
        let r = s.resumo();
        let ag = &r["agentes"][0];
        assert_eq!(ag["nome"], "Lia monta a tela");
        assert_eq!(ag["estado"], "terminou");
        assert_eq!(ag["tokens"], 3000);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn mcp_texto() {
        let t = "Checking MCP server health...\n\npixellab: https://api.pixellab.ai/mcp (HTTP) - ✓ Connected\nclaude.ai Gmail: https://x - ! Needs authentication\nfoo: node x.js - ✗ Failed to connect\n";
        let v = traduzir_mcp(t);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0]["estado"], "conectado");
        assert_eq!(v[1]["estado"], "precisa de login");
        assert_eq!(v[2]["estado"], "falhou");
    }

    #[test]
    #[ignore] // lê as sessões reais da máquina (só leitura): cargo test sessoes_reais -- --ignored --nocapture
    fn sessoes_reais() {
        let v = sessoes_vivas();
        eprintln!("pasta: {}", pasta_claude().display());
        for s in &v {
            eprintln!("pid {} {} {} transcript={:?}", s.pid, s.status, s.cwd, achar_transcript(&s.sessao, &s.cwd).is_some());
        }
        assert!(!v.is_empty());
    }

    #[test]
    fn pasta_codificada() {
        assert_eq!(pasta_do_projeto(r"a:\Claude"), "a--Claude");
    }
}
