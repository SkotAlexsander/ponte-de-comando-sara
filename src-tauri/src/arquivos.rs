//! Pasta de trabalho: listar uma pasta por vez e criar pasta ou arquivo.
//! Barreiras (a interface também valida, mas não é barreira de segurança):
//! - todo caminho é resolvido com `canonicalize` e precisa ficar dentro da pasta de trabalho
//!   (isso pega `..`, caminho absoluto de fora e atalho/junção que aponte para fora);
//! - nome validado com as regras do Windows;
//! - nunca sobrescreve: pasta com `create_dir`, arquivo com `create_new`;
//! - apagar não existe aqui.

use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const IGNORAR: [&str; 3] = ["node_modules", ".git", "target"];
pub const LIMITE_ITENS: usize = 2000;

/// Estantes da Biblioteca → pasta real, relativa à pasta PADRÃO (`A:\Claude`).
/// A Memória mora fora (`C:\Users\...\.claude\projects\a--Claude\memory`), por isso não tem pasta aqui.
pub const ESTANTES: [(&str, Option<&str>); 3] = [
    ("Cérebro", Some("02-cerebro")),
    ("Alexandria", Some(r"03-projetos\alexandria")),
    ("Memória", None),
];

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub nome: String,
    /// Caminho relativo à pasta de trabalho, com `\` ("" = a própria pasta).
    pub caminho: String,
    /// Caminho absoluto, como o Windows mostra (sem o prefixo `\\?\`).
    pub absoluto: String,
    /// "pasta" ou "arquivo".
    pub tipo: &'static str,
    /// É atalho/junção (pode apontar para fora; abrir é recusado nesse caso).
    pub link: bool,
    pub tamanho: Option<u64>,
}

#[derive(Serialize, Debug)]
pub struct Listagem {
    pub pasta: String,
    pub absoluto: String,
    pub itens: Vec<Item>,
    pub truncado: bool,
}

/// Tira o `\\?\` que o canonicalize do Windows põe na frente.
pub fn exibir(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(r) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{r}")
    } else if let Some(r) = s.strip_prefix(r"\\?\") {
        r.to_string()
    } else {
        s
    }
}

pub fn raiz_canonica(raiz: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(raiz).map_err(|_| format!("A pasta de trabalho não existe mais: {}", raiz.display()))
}

fn relativo(raiz_c: &Path, alvo_c: &Path) -> String {
    alvo_c
        .strip_prefix(raiz_c)
        .map(|r| r.to_string_lossy().replace('/', "\\"))
        .unwrap_or_default()
}

/// Valida um nome de pasta ou arquivo do Windows. Devolve a mensagem em PT-BR se não puder.
pub fn validar_nome(nome: &str) -> Result<String, String> {
    let n = nome.trim();
    if n.is_empty() {
        return Err("Escreva um nome.".into());
    }
    if n == "." || n == ".." || n.chars().any(|c| "\\/:*?\"<>|".contains(c)) {
        return Err("Esse nome não pode: sem \\ / : * ? \" < > |".into());
    }
    if n.chars().any(|c| (c as u32) < 32) {
        return Err("Esse nome tem um caractere invisível.".into());
    }
    if n.ends_with('.') || n.ends_with(' ') {
        return Err("Não pode terminar com ponto ou espaço.".into());
    }
    if n.chars().count() > 200 {
        return Err("Nome comprido demais (máximo 200 letras).".into());
    }
    let base = n.split('.').next().unwrap_or("").to_ascii_lowercase();
    let reservado = matches!(base.as_str(), "con" | "prn" | "aux" | "nul")
        || ((base.starts_with("com") || base.starts_with("lpt"))
            && base.len() == 4
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0');
    if reservado {
        return Err("O Windows reserva esse nome.".into());
    }
    Ok(n.to_string())
}

/// Resolve o `destino` que a interface manda para uma pasta real DENTRO da pasta de trabalho.
/// Aceita:
/// - caminho absoluto (precisa estar dentro da pasta de trabalho);
/// - `Biblioteca\<Estante>\...` (Cérebro, Alexandria; a Memória é recusada);
/// - caminho relativo à pasta de trabalho (`03-projetos\x\`), "" = a própria pasta.
pub fn resolver(raiz: &Path, destino: &str) -> Result<PathBuf, String> {
    let raiz_c = raiz_canonica(raiz)?;
    let d = destino.trim().replace('/', "\\");
    let d = d.trim_end_matches('\\');

    let candidato: PathBuf = if let Some(resto) = d.strip_prefix("Biblioteca\\").or(if d == "Biblioteca" { Some("") } else { None }) {
        let mut partes = resto.splitn(2, '\\');
        let estante = partes.next().unwrap_or("");
        let depois = partes.next().unwrap_or("");
        let (_, sub) = ESTANTES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(estante) || sem_acento(n) == sem_acento(estante))
            .ok_or_else(|| format!("Não conheço a estante \"{estante}\"."))?;
        let sub = sub.ok_or("A Memória fica fora da pasta de trabalho; criar nela ainda não é permitido.")?;
        let base = crate::config::Config::pasta_padrao().join(sub);
        let p = juntar_relativo(&base, depois)?;
        let base_c = fs::canonicalize(&base).map_err(|_| format!("A estante não existe: {}", base.display()))?;
        if !base_c.starts_with(&raiz_c) {
            return Err(format!(
                "A estante {estante} fica fora da pasta de trabalho atual ({}). Volte para a pasta padrão para criar ali.",
                exibir(&raiz_c)
            ));
        }
        p
    } else if Path::new(d).is_absolute() || d.starts_with("\\\\") {
        PathBuf::from(d)
    } else if d.len() >= 2 && d.as_bytes()[1] == b':' {
        // "C:algo" (relativo a um disco): não é caminho que a interface mande
        return Err("Caminho inválido.".into());
    } else {
        let direto = juntar_relativo(&raiz_c, d)?;
        // a árvore mostra a raiz pelo nome ("Claude\03-projetos\"): aceita esse nome no começo
        let nome_raiz = raiz_c.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut partes = d.splitn(2, '\\');
        let primeiro = partes.next().unwrap_or("").to_lowercase();
        if !direto.exists() && !nome_raiz.is_empty() && primeiro == nome_raiz {
            juntar_relativo(&raiz_c, partes.next().unwrap_or(""))?
        } else {
            direto
        }
    };

    let c = fs::canonicalize(&candidato).map_err(|_| format!("Essa pasta não existe: {}", candidato.display()))?;
    if !c.starts_with(&raiz_c) {
        return Err("Esse caminho fica fora da pasta de trabalho.".into());
    }
    if !c.is_dir() {
        return Err("Isso não é uma pasta.".into());
    }
    Ok(c)
}

fn sem_acento(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'é' | 'É' | 'ê' => 'e',
            'ó' | 'Ó' => 'o',
            'á' | 'Á' | 'â' | 'ã' => 'a',
            'í' => 'i',
            'ú' => 'u',
            'ç' => 'c',
            _ => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// Junta um caminho relativo recusando `..`, disco e raiz no meio.
fn juntar_relativo(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut p = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(x) => {
                let s = x.to_string_lossy();
                if s.contains(':') {
                    return Err("Caminho inválido.".into());
                }
                p.push(x)
            }
            Component::CurDir => {}
            Component::ParentDir => return Err("Caminho com \"..\" não é aceito.".into()),
            Component::RootDir | Component::Prefix(_) => return Err("Caminho inválido.".into()),
        }
    }
    Ok(p)
}

pub fn listar(raiz: &Path, pasta: &str) -> Result<Listagem, String> {
    let raiz_c = raiz_canonica(raiz)?;
    let alvo = resolver(raiz, pasta)?;
    let mut itens = Vec::new();
    let mut truncado = false;
    let leitor = fs::read_dir(&alvo).map_err(|e| format!("Não consegui abrir a pasta: {e}"))?;
    for ent in leitor.flatten() {
        let nome = ent.file_name().to_string_lossy().to_string();
        if IGNORAR.iter().any(|i| i.eq_ignore_ascii_case(&nome)) {
            continue;
        }
        let ft = match ent.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let link = ft.is_symlink();
        // segue o link só para saber se é pasta; abrir passa pelo resolver de novo
        let meta = fs::metadata(ent.path()).ok();
        let eh_pasta = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        if itens.len() >= LIMITE_ITENS {
            truncado = true;
            break;
        }
        let caminho_abs = alvo.join(&nome);
        itens.push(Item {
            caminho: relativo(&raiz_c, &caminho_abs),
            absoluto: exibir(&caminho_abs),
            nome,
            tipo: if eh_pasta { "pasta" } else { "arquivo" },
            link,
            tamanho: if eh_pasta { None } else { meta.map(|m| m.len()) },
        });
    }
    itens.sort_by(|a, b| (a.tipo != "pasta").cmp(&(b.tipo != "pasta")).then_with(|| a.nome.to_lowercase().cmp(&b.nome.to_lowercase())));
    Ok(Listagem { pasta: relativo(&raiz_c, &alvo), absoluto: exibir(&alvo), itens, truncado })
}

pub fn criar(raiz: &Path, destino: &str, nome: &str, pasta: bool) -> Result<Item, String> {
    let raiz_c = raiz_canonica(raiz)?;
    let pai = resolver(raiz, destino)?;
    let nome = validar_nome(nome)?;
    let alvo = pai.join(&nome);
    // mesmo nome com outra caixa já conta como existente no Windows; o create_* recusa
    if alvo.symlink_metadata().is_ok() {
        return Err("Já existe um item com esse nome aqui.".into());
    }
    if pasta {
        fs::create_dir(&alvo).map_err(|e| traduzir(e))?;
    } else {
        fs::OpenOptions::new().write(true).create_new(true).open(&alvo).map_err(|e| traduzir(e))?;
    }
    // conferência final: o que foi criado está dentro
    let c = fs::canonicalize(&alvo).map_err(|e| e.to_string())?;
    if !c.starts_with(&raiz_c) {
        return Err("O item criado ficou fora da pasta de trabalho (não deveria acontecer).".into());
    }
    Ok(Item {
        caminho: relativo(&raiz_c, &c),
        absoluto: exibir(&c),
        nome,
        tipo: if pasta { "pasta" } else { "arquivo" },
        link: false,
        tamanho: if pasta { None } else { Some(0) },
    })
}

/// Lê um arquivo de texto (só leitura, até 256 KB) de dentro da pasta de trabalho.
pub fn ler(raiz: &Path, caminho: &str) -> Result<String, String> {
    let c = caminho.trim().replace('/', "\\");
    let (pai, nome) = match c.rfind('\\') {
        Some(i) => (&c[..i], &c[i + 1..]),
        None => ("", c.as_str()),
    };
    let pasta = resolver(raiz, pai)?;
    let alvo = fs::canonicalize(pasta.join(nome)).map_err(|_| "Esse arquivo não existe.".to_string())?;
    if !alvo.starts_with(raiz_canonica(raiz)?) {
        return Err("Esse caminho fica fora da pasta de trabalho.".into());
    }
    let meta = fs::metadata(&alvo).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Isso não é um arquivo.".into());
    }
    let mut f = fs::File::open(&alvo).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    use std::io::Read;
    f.by_ref().take(256 * 1024).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if buf.iter().take(8000).any(|b| *b == 0) {
        return Err("Arquivo binário: não dá para mostrar aqui.".into());
    }
    let mut t = String::from_utf8_lossy(&buf).to_string();
    if meta.len() > 256 * 1024 {
        t.push_str("\n… (cortado em 256 KB)");
    }
    Ok(t)
}

fn traduzir(e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::AlreadyExists => "Já existe um item com esse nome aqui.".into(),
        std::io::ErrorKind::PermissionDenied => "O Windows não deixou criar aqui (sem permissão).".into(),
        _ => format!("Não consegui criar: {e}"),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta_teste(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ponte-arq-{nome}-{}", crate::config::aleatorio_hex(4)));
        fs::create_dir_all(d.join("sub").join("fundo")).unwrap();
        fs::write(d.join("sub").join("a.txt"), "oi").unwrap();
        fs::create_dir_all(d.join("node_modules")).unwrap();
        d
    }

    #[test]
    fn nomes() {
        assert!(validar_nome("notas.md").is_ok());
        assert!(validar_nome("  ").is_err());
        assert!(validar_nome("a/b").is_err());
        assert!(validar_nome("a:b").is_err());
        assert!(validar_nome("..").is_err());
        assert!(validar_nome("CON").is_err());
        assert!(validar_nome("nul.txt").is_err());
        assert!(validar_nome("com1").is_err());
        assert!(validar_nome("com0").is_ok());
        assert!(validar_nome("console").is_ok());
        assert!(validar_nome("fim.").is_err());
        assert!(validar_nome("fim ").is_ok()); // trim tira o espaço do fim
        assert_eq!(validar_nome(" x ").unwrap(), "x");
    }

    #[test]
    fn lista_e_ignora() {
        let d = pasta_teste("lista");
        let l = listar(&d, "").unwrap();
        let nomes: Vec<_> = l.itens.iter().map(|i| i.nome.as_str()).collect();
        assert_eq!(nomes, vec!["sub"]);
        let l2 = listar(&d, "sub").unwrap();
        assert_eq!(l2.itens[0].nome, "fundo");
        assert_eq!(l2.itens[0].tipo, "pasta");
        assert_eq!(l2.itens[1].caminho, "sub\\a.txt");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn recusa_fuga() {
        let d = pasta_teste("fuga");
        assert!(resolver(&d, "..").is_err());
        assert!(resolver(&d, "sub\\..\\..").is_err());
        assert!(resolver(&d, "C:\\Windows").is_err());
        assert!(resolver(&d, "C:Windows").is_err());
        assert!(resolver(&d, "\\\\servidor\\x").is_err());
        assert!(resolver(&d, "sub\\a.txt").is_err()); // não é pasta
        assert!(resolver(&d, &d.join("sub").display().to_string()).is_ok());
        assert!(listar(&d, "..\\").is_err());
        assert!(criar(&d, "..", "x", true).is_err());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn recusa_juncao_para_fora() {
        let d = pasta_teste("juncao");
        let fora = pasta_teste("juncao-fora");
        let ok = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(d.join("atalho"))
            .arg(&fora)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            let l = listar(&d, "").unwrap();
            assert!(l.itens.iter().any(|i| i.nome == "atalho" && i.link));
            assert!(listar(&d, "atalho").is_err());
            assert!(criar(&d, "atalho", "x.txt", false).is_err());
            assert!(!fora.join("x.txt").exists());
            // tirar só a junção (remove_dir numa junção não apaga o alvo)
            fs::remove_dir(d.join("atalho")).ok();
        } else {
            eprintln!("mklink /J indisponível; teste da junção pulado");
        }
        fs::remove_dir_all(&d).ok();
        fs::remove_dir_all(&fora).ok();
    }

    #[test]
    fn cria_sem_sobrescrever() {
        let d = pasta_teste("cria");
        let i = criar(&d, "sub", "nova", true).unwrap();
        assert_eq!(i.caminho, "sub\\nova");
        assert!(d.join("sub").join("nova").is_dir());
        let f = criar(&d, "sub\\nova\\", "notas.md", false).unwrap();
        assert_eq!(f.tipo, "arquivo");
        assert!(criar(&d, "sub", "a.txt", false).is_err());
        assert_eq!(fs::read_to_string(d.join("sub").join("a.txt")).unwrap(), "oi");
        assert!(criar(&d, "sub", "A.TXT", false).is_err()); // mesma coisa no Windows
        assert!(criar(&d, "sub", "nova", true).is_err());
        assert!(criar(&d, &d.display().to_string(), "raiz.txt", false).is_ok());
        // a árvore manda o nome da raiz na frente: "<raiz>\sub\"
        let nome = d.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(criar(&d, &format!("{nome}\\sub\\"), "pela-arvore.md", false).unwrap().caminho, "sub\\pela-arvore.md");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn le_so_dentro() {
        let d = pasta_teste("ler");
        assert_eq!(ler(&d, "sub\\a.txt").unwrap(), "oi");
        assert!(ler(&d, "..\\x.txt").is_err());
        assert!(ler(&d, "sub").is_err());
        fs::write(d.join("b.bin"), [0u8, 1, 2]).unwrap();
        assert!(ler(&d, "b.bin").is_err());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn memoria_recusada() {
        let d = pasta_teste("mem");
        let e = resolver(&d, "Biblioteca\\Memória\\").unwrap_err();
        assert!(e.contains("Memória"));
        assert!(resolver(&d, "Biblioteca\\Inexistente\\").is_err());
        fs::remove_dir_all(&d).ok();
    }
}
