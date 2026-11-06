//! O processo Node que conversa pelo Claude Agent SDK (`sidecar/ponte-sidecar.mjs`).
//! A casca abre na primeira mensagem, escreve comandos JSON no stdin e lê eventos JSON do stdout.
//! Se ele cair, a próxima mensagem abre outro (as conversas são retomadas pelo id da sessão).

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

pub type AoEvento = Arc<dyn Fn(Value) + Send + Sync>;

struct Vivo {
    filho: Child,
    entrada: ChildStdin,
}

pub struct Sidecar {
    pub script: Option<PathBuf>,
    pub node: Option<PathBuf>,
    vivo: Mutex<Option<Vivo>>,
    pasta_log: PathBuf,
}

/// Acha o node.exe por caminho absoluto (nunca pelo nome solto).
pub fn achar_node() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PONTE_NODE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            let c = d.join("node.exe");
            if c.is_file() {
                return Some(c);
            }
        }
    }
    let padrao = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
    padrao.is_file().then_some(padrao)
}

impl Sidecar {
    pub fn novo(script: Option<PathBuf>, pasta_log: &Path) -> Sidecar {
        Sidecar { script, node: achar_node(), vivo: Mutex::new(None), pasta_log: pasta_log.to_path_buf() }
    }

    pub fn disponivel(&self) -> bool {
        self.node.is_some() && self.script.as_ref().map(|s| s.is_file() && s.parent().map(|p| p.join("node_modules").join("@anthropic-ai").join("claude-agent-sdk").is_dir()).unwrap_or(false)).unwrap_or(false)
    }

    pub fn explicar_indisponivel(&self) -> String {
        if self.node.is_none() {
            "Não achei o Node (node.exe). Ele é pré-requisito para conversar pelo programa.".into()
        } else {
            "Não achei o sidecar com o Claude Agent SDK (sidecar\\node_modules).".into()
        }
    }

    fn abrir(&self, ao: AoEvento) -> Result<Vivo, String> {
        let script = self.script.clone().ok_or("sem script do sidecar")?;
        let node = self.node.clone().ok_or("sem node")?;
        let mut cmd = Command::new(&node);
        cmd.arg(&script)
            .current_dir(script.parent().unwrap_or(Path::new(".")))
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
        let mut filho = cmd.spawn().map_err(|e| format!("Não consegui abrir o sidecar: {e}"))?;
        let entrada = filho.stdin.take().ok_or("sem stdin")?;
        let saida = filho.stdout.take().ok_or("sem stdout")?;
        if let Some(mut erro) = filho.stderr.take() {
            let pasta = self.pasta_log.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = erro.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let t = String::from_utf8_lossy(&buf[..n]);
                    for l in t.lines().filter(|l| !l.trim().is_empty()) {
                        crate::config::registrar(&pasta, &format!("sidecar: {}", crate::estado::cortar(l, 400)));
                    }
                }
            });
        }
        std::thread::spawn(move || {
            for linha in BufReader::new(saida).lines() {
                let Ok(linha) = linha else { break };
                if let Ok(v) = serde_json::from_str::<Value>(&linha) {
                    ao(v);
                }
            }
            ao(serde_json::json!({ "evento": "sidecar_caiu" }));
        });
        Ok(Vivo { filho, entrada })
    }

    /// Manda um comando; abre o sidecar se ainda não estiver vivo.
    pub fn mandar(&self, ao: AoEvento, cmd: &Value) -> Result<(), String> {
        let mut g = self.vivo.lock().unwrap();
        let morto = match g.as_mut() {
            Some(v) => v.filho.try_wait().map(|s| s.is_some()).unwrap_or(true),
            None => true,
        };
        if morto {
            *g = Some(self.abrir(ao)?);
        }
        let v = g.as_mut().unwrap();
        let mut linha = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        linha.push('\n');
        if let Err(e) = v.entrada.write_all(linha.as_bytes()).and_then(|_| v.entrada.flush()) {
            *g = None;
            return Err(format!("O sidecar não respondeu: {e}"));
        }
        Ok(())
    }

    /// Só manda se estiver vivo (parar, fechar, permissão).
    pub fn mandar_se_vivo(&self, cmd: &Value) -> bool {
        let mut g = self.vivo.lock().unwrap();
        if let Some(v) = g.as_mut() {
            let mut linha = serde_json::to_string(cmd).unwrap_or_default();
            linha.push('\n');
            return v.entrada.write_all(linha.as_bytes()).and_then(|_| v.entrada.flush()).is_ok();
        }
        false
    }

    pub fn encerrar(&self) {
        let mut g = self.vivo.lock().unwrap();
        if let Some(mut v) = g.take() {
            let _ = v.entrada.write_all(b"{\"cmd\":\"sair\"}\n");
            let _ = v.entrada.flush();
            for _ in 0..10 {
                if v.filho.try_wait().map(|s| s.is_some()).unwrap_or(true) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let _ = v.filho.kill();
        }
    }
}

/// Frase do cartão de permissão, em português.
pub fn frase_permissao(nome: &str, e: &Value) -> String {
    let s = |k: &str| e.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let c = |t: String, n: usize| crate::estado::cortar(&t, n);
    match nome {
        "Bash" | "PowerShell" => format!("Rodar um comando: {}", c(s("command"), 160)),
        "Write" => format!("Escrever o arquivo {}", s("file_path")),
        "Edit" | "MultiEdit" => format!("Mudar o arquivo {}", s("file_path")),
        "NotebookEdit" => format!("Mudar o caderno {}", s("notebook_path")),
        "Read" => format!("Ler o arquivo {}", s("file_path")),
        "Glob" | "Grep" => format!("Procurar nos arquivos: {}", c(s("pattern"), 80)),
        "WebFetch" => format!("Abrir a página {}", c(s("url"), 120)),
        "WebSearch" => format!("Pesquisar na web: {}", c(s("query"), 100)),
        "Agent" | "Task" => format!("Chamar {}: {}", if s("subagent_type").is_empty() { "um funcionário".into() } else { s("subagent_type") }, c(s("description"), 100)),
        n if n.starts_with("mcp__") => {
            let partes: Vec<&str> = n.split("__").collect();
            format!("Usar {} (conexão {})", partes.last().unwrap_or(&n), partes.get(1).unwrap_or(&"?"))
        }
        n => format!("Usar a ferramenta {n}"),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn frases() {
        assert_eq!(frase_permissao("Bash", &serde_json::json!({"command":"npm test"})), "Rodar um comando: npm test");
        assert_eq!(frase_permissao("Write", &serde_json::json!({"file_path":"A:\\x.md"})), "Escrever o arquivo A:\\x.md");
        assert_eq!(frase_permissao("mcp__canva__create", &serde_json::json!({})), "Usar create (conexão canva)");
    }
}
