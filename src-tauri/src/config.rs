//! Configuração gravada em `%APPDATA%\com.pixelmartins.pontedecomando\config.json`,
//! o token do hook (`token`) e a porta em uso (`porta`).
//! Para testes, `PONTE_PASTA_DE_DADOS` troca a pasta inteira (nunca mexe na de verdade).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const PORTA_PADRAO: u16 = 47615;
pub const PASTA_PADRAO: &str = r"A:\Claude";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Porta do servidor local dos hooks (só 127.0.0.1).
    pub porta: u16,
    /// Pasta de trabalho escolhida. `None` = a padrão (`A:\Claude`).
    pub pasta_de_trabalho: Option<String>,
    /// Silenciar avisos: sem som e sem notificação do Windows (o aviso na miniatura continua).
    pub mudo: bool,
    /// "Miniatura ao minimizar": ligada = a Sara aparece no canto quando a principal sai da tela.
    pub mini_visivel: bool,
    /// Notificação do Windows: "auto" (só quando nem a principal nem a miniatura estão na tela),
    /// "sempre" ou "nunca". Fora do programa instalado nunca sai (apareceria como Windows PowerShell).
    pub toast_windows: String,
    /// Tema de display dentro do vidro da conversa: "metal" | "fosforo" | "grade".
    pub tema: String,
    /// Cor do terminal de fósforo: "ambar" | "verde" | "branco" (o branco foi o aprovado pelo Alex).
    pub fosforo: String,
    /// Letra do terminal de fósforo: "terminal" (monoespaçada) | "atual" (a da interface; a aprovada).
    pub fosforo_letra: String,
    /// "inferior-direito", "inferior-esquerdo", "superior-direito", "superior-esquerdo".
    pub mini_canto: String,
    /// Nome do monitor onde a miniatura ficou. `None` = monitor principal.
    pub mini_monitor: Option<String>,
    /// Língua da interface: "pt-BR" | "en".
    pub idioma: String,
    /// Língua das respostas: "seguir" (a da interface) | "pt-BR" | "en". Vira um append no system prompt.
    pub idioma_respostas: String,
    /// 90 | 100 | 110 | 125 (%).
    pub tamanho_letra: u32,
    /// "sistema" | "sempre" | "reduzidas".
    pub animacoes: String,
    /// Modelo, esforço e velocidade de uma conversa nova.
    pub padrao_nova: PadraoNova,
    /// Ao permitir no cartão, lembrar a regra sugerida pelo Claude Code até o fim da sessão.
    pub lembrar_aprovacoes: bool,
    pub som_aviso: bool,
    /// 0 a 100.
    pub volume_aviso: u32,
    pub abrir_com_windows: bool,
    pub comecar_na_bandeja: bool,
    /// "assinatura" (login do Claude Code) | "api" (chave guardada no Cofre do Windows).
    pub modo_conta: String,
    /// "guarda" (Controle total com guarda: a guarda do sidecar libera o que é seguro e pergunta o resto)
    /// | "sempre" (todo pedido vai para o cartão). Nunca existe modo sem canUseTool.
    pub aprovacao: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct PadraoNova {
    pub modelo: String,
    pub esforco: String,
    pub rapido: bool,
}

impl Default for PadraoNova {
    fn default() -> Self {
        PadraoNova { modelo: "opus".into(), esforco: "high".into(), rapido: false }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            porta: PORTA_PADRAO,
            pasta_de_trabalho: None,
            mudo: false,
            mini_visivel: true,
            toast_windows: "auto".into(),
            tema: "metal".into(),
            fosforo: "branco".into(),
            fosforo_letra: "atual".into(),
            mini_canto: "inferior-direito".into(),
            mini_monitor: None,
            idioma: "pt-BR".into(),
            idioma_respostas: "seguir".into(),
            tamanho_letra: 100,
            animacoes: "sistema".into(),
            padrao_nova: PadraoNova::default(),
            lembrar_aprovacoes: false,
            som_aviso: true,
            volume_aviso: 70,
            abrir_com_windows: false,
            comecar_na_bandeja: false,
            modo_conta: "assinatura".into(),
            aprovacao: "guarda".into(),
        }
    }
}

pub const TEMAS: [&str; 3] = ["metal", "fosforo", "grade"];
pub const FOSFOROS: [&str; 3] = ["ambar", "verde", "branco"];
pub const LETRAS_FOSFORO: [&str; 2] = ["terminal", "atual"];

pub fn cantos_validos() -> [&'static str; 4] {
    ["inferior-direito", "inferior-esquerdo", "superior-direito", "superior-esquerdo"]
}

impl Config {
    /// Lê a config campo a campo: um campo com tipo errado volta ao padrão SÓ ele (os outros ficam).
    /// Se algo foi descartado (ou o arquivo nem é JSON), guarda uma cópia do original antes de qualquer gravação.
    pub fn carregar(pasta: &Path) -> Config {
        let (c, descartados) = Config::carregar_relatando(pasta);
        if !descartados.is_empty() {
            registrar(pasta, &format!("config.json: campos descartados ({}); cópia guardada", descartados.join(", ")));
        }
        c
    }

    pub fn carregar_relatando(pasta: &Path) -> (Config, Vec<String>) {
        let arq = pasta.join("config.json");
        let Ok(texto) = fs::read_to_string(&arq) else { return (Config::default(), vec![]) };
        let mut descartados = vec![];
        let mut base = serde_json::to_value(Config::default()).unwrap_or_default();
        match serde_json::from_str::<serde_json::Value>(&texto) {
            Ok(serde_json::Value::Object(lido)) => {
                for (k, v) in lido {
                    let mut tentativa = base.clone();
                    tentativa[&k] = v;
                    if serde_json::from_value::<Config>(tentativa.clone()).is_ok() {
                        base = tentativa;
                    } else {
                        descartados.push(k);
                    }
                }
            }
            _ => descartados.push("(o arquivo não é JSON)".into()),
        }
        if !descartados.is_empty() {
            let copia = pasta.join(format!("config.corrompido-{}.bak", agora_ms()));
            let _ = fs::copy(&arq, &copia);
        }
        let mut c: Config = serde_json::from_value(base).unwrap_or_default();
        if !cantos_validos().contains(&c.mini_canto.as_str()) {
            c.mini_canto = "inferior-direito".into();
        }
        if !["auto", "sempre", "nunca"].contains(&c.toast_windows.as_str()) {
            c.toast_windows = "auto".into();
        }
        if !TEMAS.contains(&c.tema.as_str()) {
            c.tema = "metal".into();
        }
        if !FOSFOROS.contains(&c.fosforo.as_str()) {
            c.fosforo = Config::default().fosforo;
        }
        if !LETRAS_FOSFORO.contains(&c.fosforo_letra.as_str()) {
            c.fosforo_letra = Config::default().fosforo_letra;
        }
        if c.porta < 1024 {
            c.porta = PORTA_PADRAO;
        }
        // arquivo editado à mão com valor estranho: cada chave volta ao padrão sozinha
        let p = Config::default();
        if !["pt-BR", "en"].contains(&c.idioma.as_str()) { c.idioma = p.idioma.clone(); }
        if !["seguir", "pt-BR", "en"].contains(&c.idioma_respostas.as_str()) { c.idioma_respostas = p.idioma_respostas.clone(); }
        if ![90, 100, 110, 125].contains(&c.tamanho_letra) { c.tamanho_letra = 100; }
        if !["sistema", "sempre", "reduzidas"].contains(&c.animacoes.as_str()) { c.animacoes = p.animacoes.clone(); }
        if c.volume_aviso > 100 { c.volume_aviso = 70; }
        if !["assinatura", "api"].contains(&c.modo_conta.as_str()) { c.modo_conta = p.modo_conta.clone(); }
        if !["guarda", "sempre"].contains(&c.aprovacao.as_str()) { c.aprovacao = p.aprovacao.clone(); }
        if !["opus", "sonnet", "haiku"].contains(&c.padrao_nova.modelo.as_str()) { c.padrao_nova.modelo = "opus".into(); }
        if crate::conversa::normalizar_esforco(&c.padrao_nova.esforco).is_err() { c.padrao_nova.esforco = "high".into(); }
        (c, descartados)
    }

    pub fn salvar(&self, pasta: &Path) -> Result<(), String> {
        let texto = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        gravar_atomico(&pasta.join("config.json"), texto.as_bytes())
    }

    /// Porta efetiva: `PONTE_PORTA` (testes) vence a do arquivo.
    pub fn porta_efetiva(&self) -> u16 {
        env_teste("PONTE_PORTA")
            .and_then(|p| p.parse::<u16>().ok())
            .filter(|p| *p >= 1024)
            .unwrap_or(self.porta)
    }

    pub fn pasta_padrao() -> PathBuf {
        let p = PathBuf::from(PASTA_PADRAO);
        if p.is_dir() {
            p
        } else {
            // máquina sem o disco A: — cai para a pasta do usuário
            std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or(p)
        }
    }

    /// A pasta gravada que não existe mais (para avisar o Alex em vez de cair calado na padrão).
    pub fn pasta_gravada_sumiu(&self) -> Option<String> {
        self.pasta_de_trabalho.as_ref().filter(|p| !PathBuf::from(p).is_dir()).cloned()
    }

    /// A pasta de trabalho em uso (a escolhida, se ainda existir; senão a padrão).
    pub fn pasta_atual(&self) -> (PathBuf, bool) {
        if let Some(p) = &self.pasta_de_trabalho {
            let pb = PathBuf::from(p);
            if pb.is_dir() {
                return (pb, false);
            }
        }
        (Config::pasta_padrao(), true)
    }
}

pub fn gravar_atomico(caminho: &Path, dados: &[u8]) -> Result<(), String> {
    let tmp = caminho.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.write_all(dados).map_err(|e| e.to_string())?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, caminho).map_err(|e| format!("{}: {e}", caminho.display()))
}

/// Variáveis de ambiente de TESTE: só valem no build de desenvolvimento (no release são ignoradas).
pub fn env_teste(nome: &str) -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var(nome).ok()
    } else {
        None
    }
}

pub fn aleatorio_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::getrandom(&mut b).expect("gerador aleatório do sistema");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// UUID v4 (formato que o `claude --session-id` aceita).
pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("gerador aleatório do sistema");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Lê o token do hook; cria um novo (32 bytes aleatórios) se não houver.
pub fn garantir_token(pasta: &Path) -> Result<String, String> {
    let arq = pasta.join("token");
    if let Ok(t) = fs::read_to_string(&arq) {
        let t = t.trim().to_string();
        if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(t);
        }
    }
    let t = aleatorio_hex(32);
    gravar_atomico(&arq, t.as_bytes())?;
    Ok(t)
}

/// Troca a pasta pessoal por %USERPROFILE% (o registro não leva o nome do usuário nos caminhos).
pub fn sem_pasta_pessoal(msg: &str) -> String {
    match std::env::var("USERPROFILE") {
        Ok(u) if u.len() > 3 => {
            let mut t = msg.replace(&u, "%USERPROFILE%");
            t = t.replace(&u.replace('\\', "/"), "%USERPROFILE%");
            t
        }
        _ => msg.to_string(),
    }
}

/// Registro simples em `ponte.log` (gira em 200 KB; nunca leva conteúdo de conversa).
pub fn registrar(pasta: &Path, msg: &str) {
    let msg = &sem_pasta_pessoal(msg);
    let arq = pasta.join("ponte.log");
    if fs::metadata(&arq).map(|m| m.len() > 200_000).unwrap_or(false) {
        let _ = fs::rename(&arq, pasta.join("ponte.log.1"));
    }
    let agora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&arq) {
        let _ = writeln!(f, "[{agora}] {msg}");
    }
    if cfg!(debug_assertions) {
        eprintln!("ponte: {msg}");
    }
}

/// Registro das ações do Claude (`acoes.log`, uma linha JSON por ação; gira em 512 KB para `acoes.log.1`).
/// Só ferramenta, resumo curto (já sem segredo, ver sidecar/guarda.mjs) e decisão: nunca conteúdo.
pub fn registrar_acao(pasta: &Path, linha: &serde_json::Value) {
    let arq = pasta.join("acoes.log");
    if fs::metadata(&arq).map(|m| m.len() > 512 * 1024).unwrap_or(false) {
        let _ = fs::rename(&arq, pasta.join("acoes.log.1"));
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&arq) {
        let _ = writeln!(f, "{}", sem_pasta_pessoal(&linha.to_string()));
    }
}

/// As últimas `n` ações (a mais nova primeiro), lendo o `acoes.log` e, se faltar, o `acoes.log.1`.
pub fn ler_acoes(pasta: &Path, n: usize) -> Vec<serde_json::Value> {
    let mut v = vec![];
    for nome in ["acoes.log", "acoes.log.1"] {
        let Ok(t) = fs::read_to_string(pasta.join(nome)) else { continue };
        for l in t.lines().rev() {
            if v.len() >= n {
                return v;
            }
            if let Ok(x) = serde_json::from_str(l) {
                v.push(x);
            }
        }
    }
    v
}

pub fn agora_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn uuid_tem_formato_v4() {
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert!("89ab".contains(&u[19..20]));
    }

    #[test]
    fn token_persiste() {
        let d = std::env::temp_dir().join(format!("ponte-teste-token-{}", aleatorio_hex(4)));
        fs::create_dir_all(&d).unwrap();
        let a = garantir_token(&d).unwrap();
        let b = garantir_token(&d).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn registro_sem_pasta_pessoal() {
        if let Ok(u) = std::env::var("USERPROFILE") {
            let t = sem_pasta_pessoal(&format!("abrindo {u}\\AppData\\x"));
            assert!(!t.contains(&u));
            assert!(t.starts_with("abrindo %USERPROFILE%"));
        }
    }

    #[test]
    fn registro_de_acoes_gira_e_le_do_fim() {
        let d = std::env::temp_dir().join(format!("ponte-teste-acoes-{}", aleatorio_hex(4)));
        fs::create_dir_all(&d).unwrap();
        for i in 0..5000 {
            registrar_acao(&d, &serde_json::json!({ "ferramenta": "Bash", "resumo": format!("acao {i} {}", "x".repeat(100)), "decisao": "liberada pela guarda" }));
        }
        assert!(fs::metadata(d.join("acoes.log")).unwrap().len() <= 512 * 1024 + 300);
        assert!(d.join("acoes.log.1").is_file());
        let ult = ler_acoes(&d, 3);
        assert_eq!(ult.len(), 3);
        assert!(ult[0]["resumo"].as_str().unwrap().starts_with("acao 4999 "));
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn config_invalida_volta_ao_padrao() {
        let d = std::env::temp_dir().join(format!("ponte-teste-cfg-{}", aleatorio_hex(4)));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("config.json"), r#"{"porta": 80, "miniCanto": "meio"}"#).unwrap();
        let c = Config::carregar(&d);
        assert_eq!(c.porta, PORTA_PADRAO);
        assert_eq!(c.mini_canto, "inferior-direito");
        fs::remove_dir_all(&d).ok();
    }
}
