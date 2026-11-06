//! As configurações que a tela de CONFIGURAÇÕES lê e grava (`ponte.config`).
//! Tudo aqui é puro: valida o pedido inteiro ANTES de mudar qualquer coisa e devolve a config nova
//! mais os efeitos que a casca precisa aplicar (miniatura, autostart, sidecar…).
//! As mensagens de erro saem em PT-BR ou em inglês, conforme o `idioma` da config.

use crate::config::{Config, PadraoNova, FOSFOROS, TEMAS};
use serde_json::{json, Value};

pub const CHAVES: [&str; 18] = [
    "tema", "fosforo", "pastaTrabalho", "miniAoMinimizar", "toastWindows", "mudo",
    "idioma", "idiomaRespostas", "tamanhoLetra", "animacoes", "padraoNova", "lembrarAprovacoes",
    "somAviso", "volumeAviso", "cantoMini", "abrirComWindows", "comecarNaBandeja", "modoConta",
];

/// Texto na língua da interface.
pub fn t(idioma: &str, pt: &str, en: &str) -> String {
    if idioma == "en" { en.to_string() } else { pt.to_string() }
}

pub fn canto_curto(canto: &str) -> &'static str {
    match canto {
        "inferior-esquerdo" => "ie",
        "superior-esquerdo" => "se",
        "superior-direito" => "sd",
        _ => "id",
    }
}

fn canto_longo(curto: &str) -> Option<&'static str> {
    match curto {
        "ie" => Some("inferior-esquerdo"),
        "id" => Some("inferior-direito"),
        "se" => Some("superior-esquerdo"),
        "sd" => Some("superior-direito"),
        _ => None,
    }
}

/// O que a tela vê (nunca leva token, porta nem chave de API).
pub fn publica(c: &Config) -> Value {
    let (pasta, padrao) = c.pasta_atual();
    json!({
        "tema": c.tema,
        "fosforo": c.fosforo,
        "pastaTrabalho": crate::arquivos::exibir(&pasta),
        "pastaTrabalhoPadrao": padrao,
        "miniAoMinimizar": c.mini_visivel,
        "toastWindows": c.toast_windows,
        "mudo": c.mudo,
        "idioma": c.idioma,
        "idiomaRespostas": c.idioma_respostas,
        "tamanhoLetra": c.tamanho_letra,
        "animacoes": c.animacoes,
        "padraoNova": { "modelo": c.padrao_nova.modelo, "esforco": c.padrao_nova.esforco, "rapido": c.padrao_nova.rapido },
        "lembrarAprovacoes": c.lembrar_aprovacoes,
        "somAviso": c.som_aviso,
        "volumeAviso": c.volume_aviso,
        "cantoMini": canto_curto(&c.mini_canto),
        "abrirComWindows": c.abrir_com_windows,
        "comecarNaBandeja": c.comecar_na_bandeja,
        "modoConta": c.modo_conta,
    })
}

/// O que vai no arquivo exportado (só o que dá para importar de volta).
pub fn exportavel(c: &Config) -> Value {
    let mut v = publica(c);
    if let Some(o) = v.as_object_mut() {
        o.remove("pastaTrabalhoPadrao");
        if c.pasta_de_trabalho.is_none() {
            o.insert("pastaTrabalho".into(), Value::Null);
        }
    }
    json!({ "programa": "Ponte de Comando", "versaoConfig": 1, "config": v })
}

#[derive(Debug, Default, PartialEq)]
pub struct Efeitos {
    pub mini: bool,
    pub mudo: Option<bool>,
    pub autostart: Option<bool>,
    pub pasta: bool,
    pub canto: bool,
    /// idioma das respostas ou modo da conta mudou: a próxima mensagem abre a sessão de novo (retomando)
    pub reabrir_sidecar: bool,
}

/// A instrução que vai no fim do system prompt do Claude Code (append), ou nada.
pub fn instrucao_idioma(c: &Config) -> Option<&'static str> {
    let alvo = if c.idioma_respostas == "seguir" { c.idioma.as_str() } else { c.idioma_respostas.as_str() };
    match alvo {
        "en" => Some("Language setting chosen by the user in Ponte de Comando: write every reply in English, even when the user writes in Portuguese or another language."),
        "pt-BR" => Some("Idioma escolhido pelo usuário na Ponte de Comando: escreva toda resposta em português do Brasil, mesmo quando o usuário escrever em inglês ou outra língua."),
        _ => None,
    }
}

/// Marca curta no começo da mensagem enviada ao Claude. Medido em 06/11/2026: só o append no system prompt
/// (e até o `language` oficial) perde para a língua em que o usuário escreve; com a marca na mensagem, obedece.
pub fn marca_idioma(c: &Config) -> Option<&'static str> {
    let alvo = if c.idioma_respostas == "seguir" { c.idioma.as_str() } else { c.idioma_respostas.as_str() };
    match alvo {
        "en" => Some("[Ponte de Comando: answer in English]"),
        "pt-BR" => Some("[Ponte de Comando: responda em português do Brasil]"),
        _ => None,
    }
}

/// O valor da configuração oficial `language` do Claude Code ("idioma preferido das respostas").
pub fn language_do_claude(c: &Config) -> Option<&'static str> {
    let alvo = if c.idioma_respostas == "seguir" { c.idioma.as_str() } else { c.idioma_respostas.as_str() };
    match alvo {
        "en" => Some("english"),
        "pt-BR" => Some("brazilian portuguese"),
        _ => None,
    }
}

/// Valida `parcial` inteiro e devolve a config nova. Nada muda se der erro.
pub fn aplicar(atual: &Config, parcial: &Value) -> Result<(Config, Efeitos), String> {
    let il = atual.idioma.clone();
    let erro = |pt: &str, en: &str| t(&il, pt, en);
    let o = parcial.as_object().ok_or_else(|| erro("Mande um objeto com as chaves a mudar.", "Send an object with the keys to change."))?;
    let desconhecidas: Vec<&String> = o.keys().filter(|k| !CHAVES.contains(&k.as_str())).collect();
    if !desconhecidas.is_empty() {
        let lista = desconhecidas.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        return Err(format!("{} {lista}", erro("Chave desconhecida:", "Unknown key:")));
    }
    let mut c = atual.clone();
    let mut ef = Efeitos::default();
    let texto = |k: &str| -> Result<Option<String>, String> {
        match o.get(k) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.trim().to_string())),
            Some(_) => Err(format!("{k}: {}", t(&il, "esperava um texto.", "expected a string."))),
        }
    };
    let booleano = |k: &str| -> Result<Option<bool>, String> {
        match o.get(k) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(format!("{k}: {}", t(&il, "esperava verdadeiro ou falso.", "expected true or false."))),
        }
    };
    let um_de = |k: &str, opcoes: &[&str]| -> Result<Option<String>, String> {
        match texto(k)? {
            None => Ok(None),
            Some(v) => {
                let v2 = if k == "idioma" || k == "idiomaRespostas" { v.clone() } else { v.to_lowercase() };
                if opcoes.contains(&v2.as_str()) {
                    Ok(Some(v2))
                } else {
                    Err(format!("{k}: {} {}", t(&il, "use", "use"), opcoes.join(" | ")))
                }
            }
        }
    };

    if let Some(v) = um_de("tema", &TEMAS)? { c.tema = v; }
    if let Some(v) = um_de("fosforo", &FOSFOROS)? { c.fosforo = v; }
    if let Some(v) = um_de("toastWindows", &["auto", "sempre", "nunca"])? { c.toast_windows = v; }
    if let Some(v) = um_de("idioma", &["pt-BR", "en"])? { c.idioma = v; }
    if let Some(v) = um_de("idiomaRespostas", &["seguir", "pt-BR", "en"])? { c.idioma_respostas = v; }
    if let Some(v) = um_de("animacoes", &["sistema", "sempre", "reduzidas"])? { c.animacoes = v; }
    if let Some(v) = um_de("modoConta", &["assinatura", "api"])? {
        if v != c.modo_conta {
            ef.reabrir_sidecar = true;
        }
        c.modo_conta = v;
    }
    if let Some(v) = um_de("cantoMini", &["ie", "id", "se", "sd"])? {
        c.mini_canto = canto_longo(&v).unwrap().to_string();
        ef.canto = true;
    }
    match o.get("tamanhoLetra") {
        None => {}
        Some(v) => match v.as_u64() {
            Some(n @ (90 | 100 | 110 | 125)) => c.tamanho_letra = n as u32,
            _ => return Err(format!("tamanhoLetra: {} 90 | 100 | 110 | 125", t(&il, "use", "use"))),
        },
    }
    match o.get("volumeAviso") {
        None => {}
        Some(v) => match v.as_u64() {
            Some(n) if n <= 100 => c.volume_aviso = n as u32,
            _ => return Err(format!("volumeAviso: {}", t(&il, "de 0 a 100.", "from 0 to 100."))),
        },
    }
    if let Some(b) = booleano("miniAoMinimizar")? { c.mini_visivel = b; ef.mini = true; }
    if let Some(b) = booleano("mudo")? { c.mudo = b; ef.mudo = Some(b); }
    if let Some(b) = booleano("lembrarAprovacoes")? { c.lembrar_aprovacoes = b; }
    if let Some(b) = booleano("somAviso")? { c.som_aviso = b; }
    if let Some(b) = booleano("abrirComWindows")? { c.abrir_com_windows = b; ef.autostart = Some(b); }
    if let Some(b) = booleano("comecarNaBandeja")? { c.comecar_na_bandeja = b; }
    if let Some(p) = o.get("padraoNova") {
        let p = p.as_object().ok_or_else(|| format!("padraoNova: {}", t(&il, "esperava um objeto { modelo, esforco, rapido }.", "expected an object { modelo, esforco, rapido }.")))?;
        if let Some(k) = p.keys().find(|k| !["modelo", "esforco", "rapido"].contains(&k.as_str())) {
            return Err(format!("padraoNova.{k}: {}", t(&il, "chave desconhecida.", "unknown key.")));
        }
        let mut n: PadraoNova = c.padrao_nova.clone();
        if let Some(m) = p.get("modelo") {
            let m = m.as_str().unwrap_or("");
            n.modelo = match m.trim().to_lowercase().as_str() {
                x @ ("opus" | "sonnet" | "haiku") => x.to_string(),
                _ => return Err(format!("padraoNova.modelo: {} opus | sonnet | haiku", t(&il, "use", "use"))),
            };
        }
        if let Some(e) = p.get("esforco") {
            n.esforco = crate::conversa::normalizar_esforco(e.as_str().unwrap_or("?")).map_err(|_| format!("padraoNova.esforco: {} low | medium | high | xhigh | max", t(&il, "use", "use")))?;
        }
        if let Some(r) = p.get("rapido") {
            n.rapido = r.as_bool().ok_or_else(|| format!("padraoNova.rapido: {}", t(&il, "esperava verdadeiro ou falso.", "expected true or false.")))?;
        }
        c.padrao_nova = n;
    }
    let antes_idioma = instrucao_idioma(atual);
    match o.get("pastaTrabalho") {
        None => {}
        Some(Value::Null) => { c.pasta_de_trabalho = None; ef.pasta = true; }
        Some(Value::String(s)) => {
            let p = std::fs::canonicalize(s.trim()).map_err(|_| erro("Essa pasta não existe.", "That folder does not exist."))?;
            if !p.is_dir() {
                return Err(erro("Isso não é uma pasta.", "That is not a folder."));
            }
            c.pasta_de_trabalho = Some(crate::arquivos::exibir(&p));
            ef.pasta = true;
        }
        Some(_) => return Err(format!("pastaTrabalho: {}", t(&il, "um caminho ou null.", "a path or null."))),
    }
    if instrucao_idioma(&c) != antes_idioma {
        ef.reabrir_sidecar = true;
    }
    Ok((c, ef))
}

/// Volta tudo que a tela configura ao padrão (mantém porta, monitor e mudo da bandeja).
pub fn padrao_mantendo(atual: &Config) -> Config {
    Config { porta: atual.porta, mini_monitor: atual.mini_monitor.clone(), ..Config::default() }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn com(v: Value) -> Result<(Config, Efeitos), String> {
        aplicar(&Config::default(), &v)
    }

    #[test]
    fn chaves_novas_validas() {
        let (c, ef) = com(json!({
            "idioma": "en", "idiomaRespostas": "pt-BR", "tamanhoLetra": 125, "animacoes": "reduzidas",
            "padraoNova": { "modelo": "SONNET", "esforco": "MÉDIO", "rapido": true },
            "lembrarAprovacoes": true, "somAviso": false, "volumeAviso": 35, "cantoMini": "se",
            "abrirComWindows": true, "comecarNaBandeja": true, "modoConta": "api"
        })).unwrap();
        assert_eq!(c.idioma, "en");
        assert_eq!(c.tamanho_letra, 125);
        assert_eq!(c.padrao_nova, PadraoNova { modelo: "sonnet".into(), esforco: "medium".into(), rapido: true });
        assert_eq!(c.mini_canto, "superior-esquerdo");
        assert_eq!(c.volume_aviso, 35);
        assert_eq!(ef.autostart, Some(true));
        assert!(ef.canto && ef.reabrir_sidecar);
        let p = publica(&c);
        assert_eq!(p["cantoMini"], "se");
        assert_eq!(p["padraoNova"]["esforco"], "medium");
        assert!(p.get("porta").is_none());
    }

    #[test]
    fn recusa_valores_ruins_sem_mudar_nada() {
        for ruim in [
            json!({ "idioma": "es" }), json!({ "idiomaRespostas": "fr" }), json!({ "tamanhoLetra": 111 }),
            json!({ "animacoes": "loucas" }), json!({ "volumeAviso": 101 }), json!({ "volumeAviso": -1 }),
            json!({ "cantoMini": "meio" }), json!({ "padraoNova": { "modelo": "gpt" } }),
            json!({ "padraoNova": { "esforco": "turbo" } }), json!({ "padraoNova": { "x": 1 } }),
            json!({ "somAviso": "sim" }), json!({ "modoConta": "outra" }), json!({ "senha": "x" }),
            json!({ "tema": "fosforo", "tamanhoLetra": 7 }), // uma ruim derruba o pedido inteiro
        ] {
            assert!(com(ruim.clone()).is_err(), "deveria recusar {ruim}");
        }
    }

    #[test]
    fn mensagem_no_idioma_da_interface() {
        let mut c = Config::default();
        c.idioma = "en".into();
        let e = aplicar(&c, &json!({ "senha": 1 })).unwrap_err();
        assert!(e.starts_with("Unknown key"), "{e}");
        let e = aplicar(&Config::default(), &json!({ "senha": 1 })).unwrap_err();
        assert!(e.starts_with("Chave desconhecida"), "{e}");
    }

    #[test]
    fn idioma_das_respostas_vira_instrucao() {
        let mut c = Config::default();
        assert_eq!(instrucao_idioma(&c), Some("Idioma escolhido pelo usuário na Ponte de Comando: escreva toda resposta em português do Brasil, mesmo quando o usuário escrever em inglês ou outra língua."));
        c.idioma_respostas = "en".into();
        assert_eq!(instrucao_idioma(&c), Some("Language setting chosen by the user in Ponte de Comando: write every reply in English, even when the user writes in Portuguese or another language."));
        assert_eq!(marca_idioma(&c), Some("[Ponte de Comando: answer in English]"));
        c.idioma_respostas = "seguir".into();
        c.idioma = "en".into();
        assert_eq!(instrucao_idioma(&c), Some("Language setting chosen by the user in Ponte de Comando: write every reply in English, even when the user writes in Portuguese or another language."));
        let (_, ef) = aplicar(&Config::default(), &json!({ "idiomaRespostas": "en" })).unwrap();
        assert!(ef.reabrir_sidecar);
        let (_, ef) = aplicar(&Config::default(), &json!({ "tema": "grade" })).unwrap();
        assert!(!ef.reabrir_sidecar);
    }

    #[test]
    fn exportar_e_importar_de_volta() {
        let (c, _) = com(json!({ "tema": "grade", "volumeAviso": 40, "idioma": "en" })).unwrap();
        let ex = exportavel(&c);
        let (c2, _) = aplicar(&Config::default(), &ex["config"]).unwrap();
        assert_eq!(publica(&c2), publica(&c));
        let p = padrao_mantendo(&c2);
        assert_eq!(p.tema, "metal");
    }
}
