//! Servidor HTTP mínimo, só em 127.0.0.1, para o hook do Claude Code avisar o programa.
//! - `POST /evento` com o cabeçalho `X-Ponte-Token: <token>` e um JSON de `Evento` no corpo.
//! - `GET /saude` responde `{"ok":true}` (sem dados).
//! Recusa: outro endereço que não 127.0.0.1, pedido com `Origin` (navegador), token errado,
//! corpo acima de 64 KB e conexão lenta (3 s).

use crate::estado::Evento;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const MAX_CORPO: usize = 64 * 1024;
const MAX_CABECALHO: usize = 8 * 1024;
const MAX_CONEXOES: usize = 16;

pub type AoEvento = Arc<dyn Fn(Evento) + Send + Sync>;

pub fn iniciar(porta: u16, token: String, ao_evento: AoEvento) -> std::io::Result<u16> {
    let ouvinte = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, porta))?;
    let porta_real = ouvinte.local_addr()?.port();
    let token = Arc::new(token);
    let abertas = Arc::new(AtomicUsize::new(0));
    std::thread::Builder::new().name("ponte-servidor".into()).spawn(move || {
        for conexao in ouvinte.incoming().flatten() {
            if abertas.load(Ordering::SeqCst) >= MAX_CONEXOES {
                drop(conexao);
                continue;
            }
            let token = token.clone();
            let ao = ao_evento.clone();
            let abertas2 = abertas.clone();
            abertas.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                let _ = atender(conexao, &token, &ao);
                abertas2.fetch_sub(1, Ordering::SeqCst);
            });
        }
    })?;
    Ok(porta_real)
}

fn responder(s: &mut TcpStream, codigo: u16, texto: &str, corpo: &str) -> std::io::Result<()> {
    write!(
        s,
        "HTTP/1.1 {codigo} {texto}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{corpo}",
        corpo.len()
    )?;
    s.flush()
}

fn iguais_tempo_constante(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn atender(mut s: TcpStream, token: &str, ao: &AoEvento) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    if !s.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false) {
        return Ok(());
    }
    let mut leitor = BufReader::new(s.try_clone()?);
    let mut linha = String::new();
    let mut lidos = leitor.read_line(&mut linha)?;
    let mut partes = linha.split_whitespace();
    let metodo = partes.next().unwrap_or("").to_string();
    let caminho = partes.next().unwrap_or("").to_string();

    let mut tamanho = 0usize;
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
                "content-length" => tamanho = v.parse().unwrap_or(usize::MAX),
                "x-ponte-token" => token_recebido = v.to_string(),
                "origin" => tem_origin = true,
                _ => {}
            }
        }
    }

    match (metodo.as_str(), caminho.as_str()) {
        ("GET", "/saude") => responder(&mut s, 200, "OK", r#"{"ok":true}"#),
        ("POST", "/evento") => {
            if tem_origin {
                return responder(&mut s, 403, "Forbidden", r#"{"ok":false,"erro":"origem"}"#);
            }
            if !iguais_tempo_constante(token_recebido.as_bytes(), token.as_bytes()) {
                return responder(&mut s, 401, "Unauthorized", r#"{"ok":false,"erro":"token"}"#);
            }
            if tamanho > MAX_CORPO {
                return responder(&mut s, 413, "Payload Too Large", r#"{"ok":false}"#);
            }
            let mut corpo = vec![0u8; tamanho];
            leitor.read_exact(&mut corpo)?;
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
        c.read_to_string(&mut r).unwrap();
        r
    }

    #[test]
    fn token_e_origem() {
        let recebidos = Arc::new(Mutex::new(Vec::<Evento>::new()));
        let r2 = recebidos.clone();
        let porta = iniciar(0, "abc".into(), Arc::new(move |e| r2.lock().unwrap().push(e))).unwrap();
        let corpo = r#"{"tipo":"pronto","sessao":"x"}"#;
        let ok = pedir(porta, &format!("POST /evento HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Ponte-Token: abc\r\nContent-Length: {}\r\n\r\n{corpo}", corpo.len()));
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        let errado = pedir(porta, &format!("POST /evento HTTP/1.1\r\nX-Ponte-Token: abd\r\nContent-Length: {}\r\n\r\n{corpo}", corpo.len()));
        assert!(errado.starts_with("HTTP/1.1 401"));
        let nav = pedir(porta, &format!("POST /evento HTTP/1.1\r\nOrigin: http://mal.com\r\nX-Ponte-Token: abc\r\nContent-Length: {}\r\n\r\n{corpo}", corpo.len()));
        assert!(nav.starts_with("HTTP/1.1 403"));
        let grande = pedir(porta, "POST /evento HTTP/1.1\r\nX-Ponte-Token: abc\r\nContent-Length: 999999\r\n\r\n");
        assert!(grande.starts_with("HTTP/1.1 413"));
        let ruim = pedir(porta, "POST /evento HTTP/1.1\r\nX-Ponte-Token: abc\r\nContent-Length: 3\r\n\r\n{x}");
        assert!(ruim.starts_with("HTTP/1.1 400"));
        assert!(pedir(porta, "GET /saude HTTP/1.1\r\n\r\n").contains(r#"{"ok":true}"#));
        assert!(pedir(porta, "GET /token HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 404"));
        let v = recebidos.lock().unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].tipo, "pronto");
        assert_eq!(v[0].origem, "hook");
    }
}
