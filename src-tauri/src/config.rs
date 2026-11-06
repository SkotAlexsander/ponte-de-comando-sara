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
    pub mini_visivel: bool,
    /// "inferior-direito", "inferior-esquerdo", "superior-direito", "superior-esquerdo".
    pub mini_canto: String,
    /// Nome do monitor onde a miniatura ficou. `None` = monitor principal.
    pub mini_monitor: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            porta: PORTA_PADRAO,
            pasta_de_trabalho: None,
            mudo: false,
            mini_visivel: true,
            mini_canto: "inferior-direito".into(),
            mini_monitor: None,
        }
    }
}

pub fn cantos_validos() -> [&'static str; 4] {
    ["inferior-direito", "inferior-esquerdo", "superior-direito", "superior-esquerdo"]
}

impl Config {
    pub fn carregar(pasta: &Path) -> Config {
        let mut c: Config = fs::read_to_string(pasta.join("config.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if !cantos_validos().contains(&c.mini_canto.as_str()) {
            c.mini_canto = "inferior-direito".into();
        }
        if c.porta < 1024 {
            c.porta = PORTA_PADRAO;
        }
        c
    }

    pub fn salvar(&self, pasta: &Path) -> Result<(), String> {
        let texto = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        gravar_atomico(&pasta.join("config.json"), texto.as_bytes())
    }

    /// Porta efetiva: `PONTE_PORTA` (testes) vence a do arquivo.
    pub fn porta_efetiva(&self) -> u16 {
        std::env::var("PONTE_PORTA")
            .ok()
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

/// Registro simples em `ponte.log` (gira em 1 MB).
pub fn registrar(pasta: &Path, msg: &str) {
    let arq = pasta.join("ponte.log");
    if fs::metadata(&arq).map(|m| m.len() > 1_000_000).unwrap_or(false) {
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
