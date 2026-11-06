//! Servidor HTTP mínimo, só em 127.0.0.1, para o hook do Claude Code avisar o programa.
//! - `GET /ping` responde `{"ok":true,"app":"ponte-de-comando","id":"<id desta execução>"}`. O hook só
//!   manda o token depois de conferir esse id contra o arquivo `porta` (outro programa na porta não sabe o id).
//! - `POST /evento` com `X-Ponte-Token: <token>` e um JSON de `Evento` no corpo.
//! Recusa: outro endereço que não 127.0.0.1, pedido com `Origin` (navegador), token errado, corpo acima de 64 KB.
//! Cada conexão tem prazo TOTAL de 3 s (conexão lenta não segura o servidor) e o corpo é sempre lido
//! antes da resposta (responder sem ler faz o Windows mandar RST e apagar a resposta).

use crate::estado::Evento;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_CORPO: usize = 64 * 1024;
const MAX_DESCARTE: usize = 2 * 1024 * 1024;
const MAX_CABECALHO: usize = 8 * 1024;
const MAX_CONEXOES: usize = 64;
const PRAZO: Duration = Duration::from_secs(3);

pub type AoEvento = Arc<dyn Fn(Evento) + Send + Sync>;

/// Abre na porta pedida; se estiver ocupada, tenta as 10 seguintes.
/// Devolve a porta aberta e as tentativas que falharam (porta, erro), para o registro e o aviso.
pub fn iniciar(porta: u16, token: String, id: String, ao_evento: AoEvento) -> (Option<u16>, Vec<(u16, String)>) {
    let mut falhas = vec![];
    let candidatas: Vec<u16> = if porta == 0 { vec![0] } else { (0..=10).filter_map(|i| porta.checked_add(i)).collect() };
    for p in candidatas {
        match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, p)) {
            Ok(ouvinte) => {
                let real = ouvinte.local_addr().map(|a| a.port()).unwrap_or(p);
                ouvir(ouvinte, token, id, ao_evento);
                return (Some(real), falhas);
            }
            Err(e) => falhas.push((p, e.to_string())),
        }
    }
    (None, falhas)
}

fn ouvir(ouvinte: TcpListener, token: String, id: String, ao_evento: AoEvento) {
    let token = Arc::new(token);
    let id = Arc::new(id);
    let abertas = Arc::new(AtomicUsize::new(0));
    let _ = std::thread::Builder::new().name("ponte-servidor".into()).spawn(move || {
        for conexao in ouvinte.incoming().flatten() {
            if abertas.load(Ordering::SeqCst) >= MAX_CONEXOES {
                drop(conexao);
                continue;
            }
            let (token, id, ao, abertas2) = (token.clone(), id.clone(), ao_evento.clone(), abertas.clone());
            abertas.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                // vigia: passou do prazo total, fecha a conexão (mesmo que ela mande um byte por vez)
                let terminou = Arc::new(AtomicBool::new(false));
                if let Ok(c) = conexao.try_clone() {
                    let t = terminou.clone();
                    std::thread::spawn(move || {
                        let fim = Instant::now() + PRAZO;
                        while Instant::now() < fim {
                            if t.load(Ordering::SeqCst) {
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        let _ = c.shutdown(Shutdown::Both);
                    });
                }
                let _ = atender(conexao, &token, &id, &ao);
                terminou.store(true, Ordering::SeqCst);
                abertas2.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
}

fn responder(s: &mut TcpStream, codigo: u16, texto: &str, corpo: &str) -> std::io::Result<()> {
    write!(
        s,
        "HTTP/1.1 {codigo} {texto}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{corpo}",
        corpo.len()
    )?;
    s.flush()?;
    let _ = s.shutdown(Shutdown::Write);
    // fecha com calma (QA B5): o que o cliente ainda manda depois da resposta (corpo que não foi lido,
    // cabeçalho gigante) é lido e jogado fora por até 250 ms. Fechar com bytes não lidos faz o Windows
    // mandar RST, e o RST pode apagar a resposta antes de o cliente ler.
    let _ = s.set_read_timeout(Some(Duration::from_millis(250)));
    let fim = Instant::now() + Duration::from_millis(250);
    let mut lixo = [0u8; 16 * 1024];
    let mut total = 0usize;
    while Instant::now() < fim && total < 4 * 1024 * 1024 {
        match s.read(&mut lixo) {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
    Ok(())
}

fn iguais_tempo_constante(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn atender(mut s: TcpStream, token: &str, id: &str, ao: &AoEvento) -> std::io::Result<()> {
    s.set_read_timeout(Some(PRAZO))?;
    s.set_write_timeout(Some(PRAZO))?;
    if !s.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false) {
        return Ok(());
    }
    let mut leitor = BufReader::new(s.try_clone()?);
    let mut linha = String::new();
    let mut lidos = leitor.read_line(&mut linha)?;
    let mut partes = linha.split_whitespace();
    let metodo = partes.next().unwrap_or("").to_string();
    let caminho = partes.next().unwrap_or("").to_string();

    let mut tamanho: Option<usize> = None;
    let mut token_recebido = String::new();
    let mut tem_origin = false;
    loop {
        let mut h = String::new();
        let n = leitor.read_line(&mut h)?;
        lidos += n;
        if n == 0 || lidos > MAX_CABECALHO {
            return responder(&mut s, 431, "Request Header Fields Too Large", r#"{"ok":false}"#);
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            match k.as_str() {
                "content-length" => tamanho = Some(v.parse().unwrap_or(usize::MAX)),
                "x-ponte-token" => token_recebido = v.to_string(),
                "origin" => tem_origin = true,
                _ => {}
            }
        }
    }

    match (metodo.as_str(), caminho.as_str()) {
        ("GET", "/saude") => responder(&mut s, 200, "OK", r#"{"ok":true}"#),
        ("GET", "/ping") => responder(&mut s, 200, "OK", &format!(r#"{{"ok":true,"app":"ponte-de-comando","id":"{id}"}}"#)),
        ("POST", "/evento") => {
            let Some(tamanho) = tamanho else {
                return responder(&mut s, 400, "Bad Request", r#"{"ok":false,"erro":"sem Content-Length"}"#);
            };
            // lê o corpo ANTES de responder (até 2 MB; acima disso fecha)
            let ler = tamanho.min(MAX_DESCARTE);
            let mut corpo = Vec::with_capacity(ler.min(MAX_CORPO));
            let _ = leitor.by_ref().take(ler as u64).read_to_end(&mut corpo);
            if tem_origin {
                return responder(&mut s, 403, "Forbidden", r#"{"ok":false,"erro":"origem"}"#);
            }
            if !iguais_tempo_constante(token_recebido.as_bytes(), token.as_bytes()) {
                return responder(&mut s, 401, "Unauthorized", r#"{"ok":false,"erro":"token"}"#);
            }
            if tamanho > MAX_CORPO {
                return responder(&mut s, 413, "Payload Too Large", r#"{"ok":false}"#);
            }
            if corpo.len() < tamanho {
                return responder(&mut s, 400, "Bad Request", r#"{"ok":false,"erro":"corpo incompleto"}"#);
            }
            match serde_json::from_slice::<Evento>(&corpo) {
                Ok(mut ev) => {
                    ev.origem = "hook".into();
                    ao(ev);
                    responder(&mut s, 200, "OK", r#"{"ok":true}"#)
                }
                Err(_) => responder(&mut s, 400, "Bad Request", r#"{"ok":false,"erro":"json"}"#),
            }
        }
        _ => responder(&mut s, 404, "Not Found", r#"{"ok":false}"#),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::Mutex;

    fn pedir(porta: u16, bruto: &str) -> String {
        let mut c = TcpStream::connect(("127.0.0.1", porta)).unwrap();
        c.write_all(bruto.as_bytes()).unwrap();
        let mut r = String::new();
        let _ = c.read_to_string(&mut r);
        r
    }

    #[test]
    fn token_origem_ping_e_porta_ocupada() {
        let recebidos = Arc::new(Mutex::new(Vec::<Evento>::new()));
        let r2 = recebidos.clone();
        let (porta, _) = iniciar(0, "abc".into(), "id-1".into(), Arc::new(move |e| r2.lock().unwrap().push(e)));
        let porta = porta.unwrap();
        let corpo = r#"{"tipo":"pronto","sessao":"x"}"#;
        let ok = pedir(porta, &format!("POST /evento HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Ponte-Token: abc\r\nContent-Length: {}\r\n\r\n{corpo}", corpo.len()));
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        let errado = pedir(porta, &format!("POST /evento HTTP/1.1\r\nX-Ponte-Token: abd\r\nContent-Length: {}\r\n\r\n{corpo}", corpo.len()));
        assert!(errado.starts_with("HTTP/1.1 401"));
        let nav = pedir(porta, &format!("POST /evento HTTP/1.1\r\nOrigin: http://mal.com\r\nX-Ponte-Token: abc\r\nContent-Length: {}\r\n\r\n{corpo}", corpo.len()));
        assert!(nav.starts_with("HTTP/1.1 403"));
        let grande = "x".repeat(200 * 1024);
        let r = pedir(porta, &format!("POST /evento HTTP/1.1\r\nX-Ponte-Token: abc\r\nContent-Length: {}\r\n\r\n{grande}", grande.len()));
        assert!(r.starts_with("HTTP/1.1 413"), "{}", &r[..r.len().min(40)]);
        let ruim = pedir(porta, "POST /evento HTTP/1.1\r\nX-Ponte-Token: abc\r\nContent-Length: 3\r\n\r\n{x}");
        assert!(ruim.starts_with("HTTP/1.1 400"));
        assert!(pedir(porta, "POST /evento HTTP/1.1\r\nX-Ponte-Token: abc\r\n\r\n").starts_with("HTTP/1.1 400"));
        assert!(pedir(porta, "GET /ping HTTP/1.1\r\n\r\n").contains(r#""id":"id-1""#));
        assert!(pedir(porta, "GET /token HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 404"));
        // a mesma porta ocupada: o segundo servidor pula para a seguinte e conta a falha
        let (outra, falhas) = iniciar(porta, "t".into(), "id-2".into(), Arc::new(|_| {}));
        assert!(outra.is_some() && outra != Some(porta));
        assert_eq!(falhas[0].0, porta);
        let v = recebidos.lock().unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].origem, "hook");
    }

    #[test]
    fn conexao_lenta_nao_segura_o_servidor() {
        let (porta, _) = iniciar(0, "abc".into(), "id".into(), Arc::new(|_| {}));
        let porta = porta.unwrap();
        let mut lentas = vec![];
        for _ in 0..20 {
            let mut c = TcpStream::connect(("127.0.0.1", porta)).unwrap();
            c.write_all(b"POST /evento HTTP/1.1\r\n").unwrap();
            lentas.push(c);
        }
        let t0 = Instant::now();
        let r = pedir(porta, "GET /saude HTTP/1.1\r\n\r\n");
        assert!(r.contains("ok"), "{r}");
        assert!(t0.elapsed() < Duration::from_secs(4));
    }
}
