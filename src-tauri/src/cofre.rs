//! Chave de API da Anthropic no Cofre do Windows (Gerenciador de Credenciais), por CredWriteW/CredReadW/CredDeleteW.
//! Regras: a chave nunca vai para arquivo, log, linha de comando nem evento para as janelas.
//! Ela só sai daqui para a variável de ambiente ANTHROPIC_API_KEY do processo filho (sidecar/teste).

pub const ALVO_PADRAO: &str = "PonteDeComando/anthropic-api-key";

/// O nome da credencial. Testes podem trocar por `PONTE_COFRE_ALVO`, mas só para nomes de teste.
pub fn alvo() -> String {
    match std::env::var("PONTE_COFRE_ALVO") {
        Ok(a) if a.starts_with("PonteDeComando-teste") => a,
        _ => ALVO_PADRAO.to_string(),
    }
}

/// Formato de uma chave da Anthropic (sem conferir se ela vale: isso é o testarChave).
pub fn formato_valido(chave: &str) -> bool {
    chave.starts_with("sk-ant-") && (30..=300).contains(&chave.len()) && chave.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn largo(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn guardar(alvo: &str, chave: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};
    let mut nome = largo(alvo);
    let mut usuario = largo("ponte-de-comando");
    let mut dados = chave.as_bytes().to_vec();
    let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
    cred.Type = CRED_TYPE_GENERIC;
    cred.TargetName = nome.as_mut_ptr();
    cred.UserName = usuario.as_mut_ptr();
    cred.CredentialBlobSize = dados.len() as u32;
    cred.CredentialBlob = dados.as_mut_ptr();
    cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
    let ok = unsafe { CredWriteW(&cred, 0) } != 0;
    dados.iter_mut().for_each(|b| *b = 0);
    if ok { Ok(()) } else { Err(format!("o Windows recusou gravar no Cofre (erro {})", std::io::Error::last_os_error())) }
}

#[cfg(windows)]
pub fn ler(alvo: &str) -> Option<String> {
    use windows_sys::Win32::Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC};
    let nome = largo(alvo);
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        if CredReadW(nome.as_ptr(), CRED_TYPE_GENERIC, 0, &mut p) == 0 || p.is_null() {
            return None;
        }
        let c = &*p;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
        CredFree(p as *const core::ffi::c_void);
        String::from_utf8(bytes).ok()
    }
}

#[cfg(windows)]
pub fn apagar(alvo: &str) -> bool {
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};
    let nome = largo(alvo);
    unsafe { CredDeleteW(nome.as_ptr(), CRED_TYPE_GENERIC, 0) != 0 }
}

#[cfg(not(windows))]
pub fn guardar(_alvo: &str, _chave: &str) -> Result<(), String> { Err("só no Windows".into()) }
#[cfg(not(windows))]
pub fn ler(_alvo: &str) -> Option<String> { None }
#[cfg(not(windows))]
pub fn apagar(_alvo: &str) -> bool { false }

pub fn existe(alvo: &str) -> bool {
    ler(alvo).is_some()
}

/// "s***@gmail.com" (a tela não precisa do e-mail inteiro).
pub fn mascarar_email(e: &str) -> String {
    match e.split_once('@') {
        Some((nome, dominio)) if !nome.is_empty() => format!("{}***@{}", nome.chars().next().unwrap(), dominio),
        _ => "***".into(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn formato() {
        assert!(formato_valido("sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123"));
        assert!(!formato_valido("sk-abc"));
        assert!(!formato_valido("sk-ant-com espaço-ABCDEFGHIJKLMNOPQRS"));
        assert!(!formato_valido("chave qualquer"));
    }

    #[test]
    fn email_mascarado() {
        assert_eq!(mascarar_email("skot@exemplo.com"), "s***@exemplo.com");
        assert_eq!(mascarar_email("sem-arroba"), "***");
    }

    #[test]
    fn alvo_so_troca_para_nome_de_teste() {
        std::env::set_var("PONTE_COFRE_ALVO", "OutroPrograma/segredo");
        assert_eq!(alvo(), ALVO_PADRAO);
        std::env::set_var("PONTE_COFRE_ALVO", "PonteDeComando-teste-x");
        assert_eq!(alvo(), "PonteDeComando-teste-x");
        std::env::remove_var("PONTE_COFRE_ALVO");
    }

    #[test]
    fn guarda_le_e_apaga_com_credencial_de_teste() {
        // credencial com nome próprio de teste, apagada no fim — nunca a do Alex
        let alvo = format!("PonteDeComando-teste-unit-{}", crate::config::aleatorio_hex(4));
        let chave = "sk-ant-teste-0000000000000000000000000000";
        assert!(ler(&alvo).is_none());
        guardar(&alvo, chave).unwrap();
        assert_eq!(ler(&alvo).as_deref(), Some(chave));
        assert!(apagar(&alvo));
        assert!(ler(&alvo).is_none());
        assert!(!apagar(&alvo));
    }
}
