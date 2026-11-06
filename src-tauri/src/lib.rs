//! Ponte de Comando · Sara — a casca nativa (Tauri 2).
//! Janelas: "carregamento" (abre primeiro, ≥ 8 s), "principal" (Main/Fluxo) e "mini" (Sara no canto).
//! O resto: bandeja, servidor local dos hooks, pasta de trabalho, conversas pelo sidecar do SDK
//! e o motor de tokens (leitura dos transcripts do Claude Code).

mod arquivos;
mod config;
mod conversa;
mod estado;
mod servidor;
mod sidecar;
mod tokens;

use config::{registrar, Config};
use conversa::{Conversas, InfoConversa, Pedido, Resposta};
use estado::{Estado, Evento, Memoria};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

const PONTE_JS: &str = include_str!("../../ui/ponte.js");
const MAIN_JS: &str = include_str!("../../ui/ponte-main.js");
const MINIMO_CARREGAMENTO: Duration = Duration::from_secs(8);
const MARGEM_MINI: f64 = 16.0;

pub struct Ponte {
    pasta_dados: PathBuf,
    config: Mutex<Config>,
    estado: Mutex<Estado>,
    memoria: Mutex<Memoria>,
    conversas: Arc<Conversas>,
    sidecar: sidecar::Sidecar,
    /// pedido de permissão → conversa
    permissoes: Mutex<HashMap<String, String>>,
    /// conversa → título do último pedido (para o aviso de pronto)
    pedidos: Mutex<HashMap<String, String>>,
    inicio: Instant,
    concluido: AtomicBool,
    arrastando: AtomicBool,
    ultimo_movimento: Mutex<Instant>,
    porta: Mutex<Option<u16>>,
    item_mudo: Mutex<Option<CheckMenuItem<Wry>>>,
    item_mini: Mutex<Option<MenuItem<Wry>>>,
    motor_tokens: Arc<tokens::Motor>,
}

fn ponte(app: &AppHandle) -> tauri::State<'_, Ponte> {
    app.state::<Ponte>()
}

fn raiz(app: &AppHandle) -> PathBuf {
    ponte(app).config.lock().unwrap().pasta_atual().0
}

fn salvar_config(app: &AppHandle) {
    let p = ponte(app);
    let c = p.config.lock().unwrap().clone();
    if let Err(e) = c.salvar(&p.pasta_dados) {
        registrar(&p.pasta_dados, &format!("não gravei a config: {e}"));
    }
}

// ---------------------------------------------------------------- estado e avisos

fn receber_evento(app: &AppHandle, ev: Evento) {
    let p = ponte(app);
    let mudo = p.config.lock().unwrap().mudo;
    let (copia, notificar) = {
        let mut e = p.estado.lock().unwrap();
        let mut m = p.memoria.lock().unwrap();
        let n = estado::aplicar(&mut e, &mut m, &ev);
        e.mudo = mudo;
        (e.clone(), n)
    };
    let _ = app.emit("ponte:estado", &copia);
    if let Some(n) = notificar {
        if !mudo {
            if let Err(err) = app.notification().builder().title(&n.titulo).body(&n.corpo).show() {
                registrar(&p.pasta_dados, &format!("notificação falhou: {err}"));
            }
        }
    }
}

#[tauri::command]
fn estado_atual(app: AppHandle) -> Estado {
    let p = ponte(&app);
    let mut e = p.estado.lock().unwrap().clone();
    e.mudo = p.config.lock().unwrap().mudo;
    e.som = false;
    e
}

#[tauri::command]
fn dispensar_aviso(app: AppHandle) {
    let p = ponte(&app);
    let copia = {
        let mut e = p.estado.lock().unwrap();
        estado::dispensar(&mut e);
        e.mudo = p.config.lock().unwrap().mudo;
        e.clone()
    };
    let _ = app.emit("ponte:estado", &copia);
}

fn aplicar_mudo(app: &AppHandle, mudo: bool) {
    let p = ponte(app);
    p.config.lock().unwrap().mudo = mudo;
    salvar_config(app);
    let item = p.item_mudo.lock().unwrap();
    if let Some(i) = item.as_ref() {
        let _ = i.set_checked(mudo);
    }
    drop(item);
    let _ = app.emit("ponte:mudo", mudo);
}

#[tauri::command]
fn definir_mudo(app: AppHandle, mudo: bool) {
    aplicar_mudo(&app, mudo);
}

#[tauri::command]
fn obter_mudo(app: AppHandle) -> bool {
    ponte(&app).config.lock().unwrap().mudo
}

// ---------------------------------------------------------------- janelas

fn argumentos_webview() -> String {
    // os padrões do wry + som sem clique (o aviso de pronto toca sozinho)
    let mut a = String::from("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required");
    if let Ok(porta) = std::env::var("PONTE_DEPURAR_PORTA") {
        if porta.parse::<u16>().is_ok() {
            a.push_str(&format!(" --remote-debugging-port={porta}"));
        }
    }
    a
}

fn criar_janelas(app: &AppHandle) -> tauri::Result<()> {
    let args = argumentos_webview();
    WebviewWindowBuilder::new(app, "carregamento", WebviewUrl::App("carregamento.html".into()))
        .title("Ponte de Comando")
        .inner_size(960.0, 560.0)
        .resizable(false)
        .decorations(false)
        .center()
        .skip_taskbar(false)
        .initialization_script(PONTE_JS)
        .additional_browser_args(&args)
        .visible(true)
        .build()?;

    WebviewWindowBuilder::new(app, "principal", WebviewUrl::App("estudio/Main.dc.html".into()))
        .title("Ponte de Comando · Sara")
        .inner_size(1600.0, 900.0)
        .min_inner_size(960.0, 600.0)
        .center()
        .initialization_script(PONTE_JS)
        .initialization_script(MAIN_JS)
        .additional_browser_args(&args)
        .visible(false)
        .build()?;

    WebviewWindowBuilder::new(app, "mini", WebviewUrl::App("mini.html".into()))
        .title("Sara")
        .inner_size(380.0, 300.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .initialization_script(PONTE_JS)
        .additional_browser_args(&args)
        .visible(false)
        .build()?;
    Ok(())
}

fn mostrar_principal_interno(app: &AppHandle) {
    if let Some(j) = app.get_webview_window("principal") {
        let _ = j.show();
        let _ = j.unminimize();
        let _ = j.set_focus();
    }
}

#[tauri::command]
fn mostrar_principal(app: AppHandle) {
    if ponte(&app).concluido.load(Ordering::SeqCst) {
        mostrar_principal_interno(&app);
    }
}

fn concluir_carregamento(app: &AppHandle) {
    let p = ponte(app);
    if p.concluido.swap(true, Ordering::SeqCst) {
        return;
    }
    let falta = MINIMO_CARREGAMENTO.saturating_sub(p.inicio.elapsed());
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(falta);
        mostrar_principal_interno(&app);
        if ponte(&app).config.lock().unwrap().mini_visivel {
            mostrar_mini(&app, true);
        }
        if let Some(c) = app.get_webview_window("carregamento") {
            let _ = c.close();
        }
        registrar(&ponte(&app).pasta_dados, "carregamento concluído");
    });
}

#[tauri::command]
fn carregamento_concluido(app: AppHandle) {
    concluir_carregamento(&app);
}

// ---------------------------------------------------------------- miniatura

fn monitor_da_mini(app: &AppHandle) -> Option<tauri::Monitor> {
    let nome = ponte(app).config.lock().unwrap().mini_monitor.clone();
    if let Some(n) = nome {
        if let Ok(ms) = app.available_monitors() {
            if let Some(m) = ms.into_iter().find(|m| m.name() == Some(&n)) {
                return Some(m);
            }
        }
    }
    app.primary_monitor().ok().flatten()
}

/// Onde a miniatura deve ficar: o canto gravado, dentro da área útil (acima da barra de tarefas).
fn posicao_esperada(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    let mini = app.get_webview_window("mini")?;
    let m = monitor_da_mini(app)?;
    let wa = m.work_area();
    let tam = mini.outer_size().ok()?;
    let margem = (MARGEM_MINI * m.scale_factor()).round() as i32;
    let canto = ponte(app).config.lock().unwrap().mini_canto.clone();
    let x = if canto.ends_with("direito") { wa.position.x + wa.size.width as i32 - tam.width as i32 - margem } else { wa.position.x + margem };
    let y = if canto.starts_with("inferior") { wa.position.y + wa.size.height as i32 - tam.height as i32 - margem } else { wa.position.y + margem };
    Some(PhysicalPosition::new(x, y))
}

fn posicionar_mini(app: &AppHandle) {
    if let (Some(mini), Some(pos)) = (app.get_webview_window("mini"), posicao_esperada(app)) {
        let _ = mini.set_position(pos);
    }
}

fn mostrar_mini(app: &AppHandle, mostrar: bool) {
    let p = ponte(app);
    p.config.lock().unwrap().mini_visivel = mostrar;
    salvar_config(app);
    if let Some(mini) = app.get_webview_window("mini") {
        if mostrar {
            posicionar_mini(app);
            let _ = mini.show();
            let _ = mini.set_always_on_top(true);
        } else {
            let _ = mini.hide();
        }
    }
    let item = p.item_mini.lock().unwrap();
    if let Some(i) = item.as_ref() {
        let _ = i.set_text(if mostrar { "Esconder a miniatura" } else { "Mostrar a miniatura" });
    }
}

#[tauri::command]
fn mini_tamanho(app: AppHandle, largura: f64, altura: f64) {
    if let Some(mini) = app.get_webview_window("mini") {
        let l = largura.clamp(80.0, 600.0).ceil();
        let a = altura.clamp(80.0, 700.0).ceil();
        let _ = mini.set_size(LogicalSize::new(l, a));
        if !ponte(&app).arrastando.load(Ordering::SeqCst) {
            posicionar_mini(&app);
        }
    }
}

#[cfg(windows)]
fn botao_esquerdo_apertado() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
}
#[cfg(not(windows))]
fn botao_esquerdo_apertado() -> bool {
    false
}

/// A miniatura começou a ser arrastada. O fim é percebido aqui (botão solto e janela parada),
/// porque no Windows o startDragging pode voltar antes de o Alex soltar o mouse.
#[tauri::command]
fn mini_arrastando(app: AppHandle) {
    let p = ponte(&app);
    if p.arrastando.swap(true, Ordering::SeqCst) {
        return;
    }
    *p.ultimo_movimento.lock().unwrap() = Instant::now();
    std::thread::spawn(move || {
        let inicio = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(120));
            let parado = ponte(&app).ultimo_movimento.lock().unwrap().elapsed() > Duration::from_millis(250);
            if (parado && !botao_esquerdo_apertado()) || inicio.elapsed() > Duration::from_secs(60) {
                break;
            }
        }
        finalizar_arrasto(&app);
    });
}

/// Chamado pela página quando o arrasto termina; o fim de verdade é do vigia em mini_arrastando.
#[tauri::command]
fn mini_soltou(app: AppHandle) -> String {
    ponte(&app).config.lock().unwrap().mini_canto.clone()
}

/// Depois de arrastar: gruda no canto mais perto, no monitor onde ela ficou, e grava.
fn finalizar_arrasto(app: &AppHandle) -> String {
    let app = app.clone();
    let p = ponte(&app);
    p.arrastando.store(false, Ordering::SeqCst);
    let Some(mini) = app.get_webview_window("mini") else { return String::new() };
    let (Ok(pos), Ok(tam)) = (mini.outer_position(), mini.outer_size()) else { return String::new() };
    let cx = pos.x as f64 + tam.width as f64 / 2.0;
    let cy = pos.y as f64 + tam.height as f64 / 2.0;
    let m = app.monitor_from_point(cx, cy).ok().flatten().or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = m {
        let wa = m.work_area();
        let meio_x = wa.position.x as f64 + wa.size.width as f64 / 2.0;
        let meio_y = wa.position.y as f64 + wa.size.height as f64 / 2.0;
        let canto = format!("{}-{}", if cy >= meio_y { "inferior" } else { "superior" }, if cx >= meio_x { "direito" } else { "esquerdo" });
        {
            let mut c = p.config.lock().unwrap();
            c.mini_canto = canto.clone();
            c.mini_monitor = m.name().cloned();
        }
        salvar_config(&app);
        posicionar_mini(&app);
        let _ = app.emit("ponte:mini-canto", &canto);
        return canto;
    }
    String::new()
}

#[tauri::command]
fn mini_canto(app: AppHandle) -> String {
    ponte(&app).config.lock().unwrap().mini_canto.clone()
}

// ---------------------------------------------------------------- pasta de trabalho

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InfoPasta {
    caminho: String,
    nome: String,
    padrao: bool,
    caminho_padrao: String,
}

fn info_pasta(app: &AppHandle) -> InfoPasta {
    let (p, padrao) = ponte(app).config.lock().unwrap().pasta_atual();
    InfoPasta {
        caminho: arquivos::exibir(&p),
        nome: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| arquivos::exibir(&p)),
        padrao,
        caminho_padrao: arquivos::exibir(&Config::pasta_padrao()),
    }
}

#[tauri::command]
fn pasta_de_trabalho(app: AppHandle) -> InfoPasta {
    info_pasta(&app)
}

#[tauri::command]
fn listar(app: AppHandle, pasta: Option<String>) -> Result<arquivos::Listagem, String> {
    arquivos::listar(&raiz(&app), pasta.as_deref().unwrap_or(""))
}

#[tauri::command]
fn criar_pasta(app: AppHandle, destino: String, nome: String) -> Result<arquivos::Item, String> {
    let r = arquivos::criar(&raiz(&app), &destino, &nome, true);
    registrar(&ponte(&app).pasta_dados, &format!("criar pasta em {destino}: {}", if r.is_ok() { "ok" } else { "recusado" }));
    r
}

#[tauri::command]
fn criar_arquivo(app: AppHandle, destino: String, nome: String) -> Result<arquivos::Item, String> {
    let r = arquivos::criar(&raiz(&app), &destino, &nome, false);
    registrar(&ponte(&app).pasta_dados, &format!("criar arquivo em {destino}: {}", if r.is_ok() { "ok" } else { "recusado" }));
    r
}

#[tauri::command]
fn ler_arquivo(app: AppHandle, caminho: String) -> Result<String, String> {
    arquivos::ler(&raiz(&app), &caminho)
}

#[tauri::command]
async fn trocar_pasta(app: AppHandle) -> Result<Option<String>, String> {
    let atual = raiz(&app);
    let app2 = app.clone();
    let escolha = tauri::async_runtime::spawn_blocking(move || {
        let mut d = app2.dialog().file().set_title("Escolha a pasta de trabalho").set_directory(&atual);
        if let Some(j) = app2.get_webview_window("principal") {
            d = d.set_parent(&j);
        }
        d.blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(escolha) = escolha else { return Ok(None) };
    let caminho = escolha.into_path().map_err(|e| e.to_string())?;
    let c = std::fs::canonicalize(&caminho).map_err(|_| "Essa pasta não existe.".to_string())?;
    if !c.is_dir() {
        return Err("Isso não é uma pasta.".into());
    }
    let texto = arquivos::exibir(&c);
    ponte(&app).config.lock().unwrap().pasta_de_trabalho = Some(texto.clone());
    salvar_config(&app);
    let _ = app.emit("ponte:pasta", info_pasta(&app));
    Ok(Some(texto))
}

#[tauri::command]
fn voltar_padrao(app: AppHandle) -> InfoPasta {
    ponte(&app).config.lock().unwrap().pasta_de_trabalho = None;
    salvar_config(&app);
    let i = info_pasta(&app);
    let _ = app.emit("ponte:pasta", &i);
    i
}

// ---------------------------------------------------------------- conversas

fn emitir_conversa(app: &AppHandle, v: &Value) {
    let _ = app.emit("ponte:conversa", v);
}

fn ao_sidecar(app: &AppHandle, mut v: Value) {
    let p = ponte(app);
    let evento = v.get("evento").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let conversa = v.get("conversa").and_then(|x| x.as_str()).unwrap_or("").to_string();
    match evento.as_str() {
        "conversa" => {
            let tipo = v.get("tipo").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let projeto = p.conversas.info(&conversa).map(|i| PathBuf::from(i.pasta).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).unwrap_or_default();
            match tipo.as_str() {
                "inicio" => p.conversas.marcar_iniciada(&conversa),
                "ferramenta" => {
                    let nome = v.get("nome").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let (leg, det) = conversa::legenda_ferramenta(&nome, v.get("entrada").unwrap_or(&Value::Null));
                    v["legenda"] = json!(leg);
                    v["detalhe"] = json!(det);
                    if let Some(o) = v.as_object_mut() {
                        o.remove("entrada"); // a janela não precisa do conteúdo
                    }
                    receber_evento(app, Evento { tipo: "trabalhando".into(), fase: "ferramenta".into(), projeto, ferramenta: nome, legenda: leg, origem: "programa".into(), ..Default::default() });
                }
                "resultado" => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    let erro = v.get("erro").and_then(|x| x.as_bool()).unwrap_or(false);
                    let texto = v.get("texto").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let titulo = p.pedidos.lock().unwrap().get(&conversa).cloned().unwrap_or_default();
                    receber_evento(app, Evento { tipo: if erro { "erro".into() } else { "pronto".into() }, projeto, titulo: if erro { String::new() } else { titulo }, texto: if erro { texto.clone() } else { String::new() }, resumo: texto, origem: "programa".into(), ..Default::default() });
                    p.motor_tokens.cutucar();
                }
                "erro" => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    let texto = v.get("texto").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    receber_evento(app, Evento { tipo: "erro".into(), projeto, texto, origem: "programa".into(), ..Default::default() });
                }
                "cancelado" => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    receber_evento(app, Evento { tipo: "esperando".into(), origem: "programa".into(), ..Default::default() });
                }
                "fechada" => p.conversas.marcar_rodando_sdk(&conversa, false),
                _ => {}
            }
            emitir_conversa(app, &v);
        }
        "permissao" => {
            let pedido = v.get("pedido").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let ferramenta = v.get("ferramenta").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let entrada = v.get("entrada").cloned().unwrap_or(Value::Null);
            let frase = sidecar::frase_permissao(&ferramenta, &entrada);
            let detalhe = estado::cortar(&serde_json::to_string_pretty(&entrada).unwrap_or_default(), 1500);
            p.permissoes.lock().unwrap().insert(pedido.clone(), conversa.clone());
            let _ = app.emit("ponte:permissao", json!({ "conversa": conversa, "pedido": pedido, "ferramenta": ferramenta, "frase": frase, "detalhe": detalhe }));
            receber_evento(app, Evento { tipo: "precisa".into(), texto: format!("Pedido de permissão: {frase}"), origem: "programa".into(), ..Default::default() });
        }
        "permissao_fim" => {
            let pedido = v.get("pedido").and_then(|x| x.as_str()).unwrap_or("").to_string();
            p.permissoes.lock().unwrap().remove(&pedido);
            let _ = app.emit("ponte:permissao-fim", json!({ "conversa": conversa, "pedido": pedido }));
        }
        "sidecar_caiu" => {
            registrar(&p.pasta_dados, "o sidecar saiu");
            for c in p.conversas.limpar_rodando_sdk() {
                emitir_conversa(app, &json!({ "evento": "conversa", "conversa": c, "tipo": "erro", "texto": "O processo da conversa fechou. A próxima mensagem retoma a mesma conversa." }));
            }
            let pend: Vec<(String, String)> = p.permissoes.lock().unwrap().drain().collect();
            for (pedido, c) in pend {
                let _ = app.emit("ponte:permissao-fim", json!({ "conversa": c, "pedido": pedido }));
            }
        }
        "pronto_para_ouvir" => registrar(&p.pasta_dados, &format!("sidecar pronto: {v}")),
        _ => {}
    }
}

fn ao_sidecar_cb(app: &AppHandle) -> sidecar::AoEvento {
    let a = app.clone();
    Arc::new(move |v| ao_sidecar(&a, v))
}

#[tauri::command]
fn enviar(app: AppHandle, pedido: Pedido) -> Result<Resposta, String> {
    let p = ponte(&app);
    let raiz = raiz(&app);
    let texto = conversa::validar_texto(&pedido.texto)?;
    p.pedidos.lock().unwrap().insert(pedido.conversa.clone(), estado::cortar(&texto, 140));
    if !p.sidecar.disponivel() {
        registrar(&p.pasta_dados, &format!("conversa pelo claude -p (sem sidecar): {}", p.sidecar.explicar_indisponivel()));
        let a1 = app.clone();
        let a2 = app.clone();
        let saida: conversa::Saida = Arc::new(move |ev| {
            let mut v = serde_json::to_value(&ev).unwrap_or(Value::Null);
            v["evento"] = json!("conversa");
            emitir_conversa(&a1, &v);
        });
        let ao: conversa::AoEstado = Arc::new(move |ev| receber_evento(&a2, ev));
        return p.conversas.enviar(pedido, &raiz, saida, ao);
    }
    if p.conversas.rodando(&pedido.conversa) {
        return Err("Essa conversa ainda está trabalhando. Espere terminar ou aperte PARAR.".into());
    }
    let ja = p.conversas.info(&pedido.conversa);
    let pasta = match &ja {
        Some(i) => i.pasta.clone(),
        None => arquivos::exibir(&arquivos::resolver(&raiz, &pedido.pasta)?),
    };
    let info = p.conversas.preparar(&pedido, pasta)?;
    if !PathBuf::from(&info.pasta).is_dir() {
        return Err(format!("A pasta desta conversa não existe mais: {}", info.pasta));
    }
    p.conversas.marcar_rodando_sdk(&pedido.conversa, true);
    let projeto = PathBuf::from(&info.pasta).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    receber_evento(&app, Evento { tipo: "trabalhando".into(), fase: "pedido".into(), projeto, titulo: estado::cortar(&texto, 140), origem: "programa".into(), ..Default::default() });
    let cmd = json!({
        "cmd": "enviar", "conversa": pedido.conversa, "texto": texto,
        "modelo": info.modelo, "esforco": info.esforco, "rapido": info.rapido,
        "cwd": info.pasta, "sessao": info.sessao, "retomar": info.iniciada
    });
    if let Err(e) = p.sidecar.mandar(ao_sidecar_cb(&app), &cmd) {
        p.conversas.marcar_rodando_sdk(&pedido.conversa, false);
        receber_evento(&app, Evento { tipo: "erro".into(), texto: e.clone(), origem: "programa".into(), ..Default::default() });
        return Err(e);
    }
    Ok(Resposta { conversa: pedido.conversa, nova: !info.iniciada, sessao: info.sessao, modelo: info.modelo, esforco: info.esforco, rapido: info.rapido, pasta: info.pasta, motor: "sdk" })
}

#[tauri::command]
fn parar(app: AppHandle, conversa: String) -> bool {
    let p = ponte(&app);
    if p.sidecar.mandar_se_vivo(&json!({ "cmd": "parar", "conversa": conversa })) {
        return true;
    }
    p.conversas.cancelar(&conversa)
}

#[tauri::command]
fn responder_permissao(app: AppHandle, pedido: String, permitir: bool, motivo: Option<String>) -> Result<(), String> {
    let p = ponte(&app);
    if p.permissoes.lock().unwrap().remove(&pedido).is_none() {
        return Err("Esse pedido já foi respondido ou cancelado.".into());
    }
    let ok = p.sidecar.mandar_se_vivo(&json!({ "cmd": "permissao", "pedido": pedido, "decisao": if permitir { "permitir" } else { "negar" }, "motivo": motivo }));
    registrar(&p.pasta_dados, &format!("permissão {pedido}: {}", if permitir { "permitida" } else { "negada" }));
    let _ = app.emit("ponte:permissao-fim", json!({ "pedido": pedido }));
    dispensar_aviso(app.clone());
    if ok { Ok(()) } else { Err("A conversa não está mais aberta.".into()) }
}

#[tauri::command]
fn parar_agente(app: AppHandle, conversa: String, tarefa: String) -> bool {
    ponte(&app).sidecar.mandar_se_vivo(&json!({ "cmd": "parar_agente", "conversa": conversa, "tarefa": tarefa }))
}

#[tauri::command]
fn conversa_info(app: AppHandle, conversa: String) -> Option<InfoConversa> {
    ponte(&app).conversas.info(&conversa)
}

#[tauri::command]
fn info_app(app: AppHandle) -> Value {
    let p = ponte(&app);
    json!({
        "versao": app.package_info().version.to_string(),
        "porta": *p.porta.lock().unwrap(),
        "pastaDeDados": arquivos::exibir(&p.pasta_dados),
        "motorConversa": if p.sidecar.disponivel() { "sdk" } else { "cli" },
        "node": p.sidecar.node.as_ref().map(|n| arquivos::exibir(n)),
        "janelas": {
            "carregamento": app.get_webview_window("carregamento").and_then(|j| j.is_visible().ok()),
            "principal": app.get_webview_window("principal").and_then(|j| j.is_visible().ok()),
            "mini": app.get_webview_window("mini").and_then(|j| j.is_visible().ok()),
        },
    })
}

// ---------------------------------------------------------------- tokens, sessões, limites, conta

#[tauri::command]
fn tokens_resumo(app: AppHandle) -> Value {
    ponte(&app).motor_tokens.resumo()
}

#[tauri::command]
fn limites(app: AppHandle) -> Value {
    ponte(&app).motor_tokens.limites()
}

#[tauri::command]
fn conversas_antigas(app: AppHandle, pasta: Option<String>, limite: Option<usize>) -> Value {
    let alvo = pasta.map(PathBuf::from).unwrap_or_else(|| raiz(&app));
    tokens::conversas_antigas(&alvo, limite.unwrap_or(50))
}

#[tauri::command]
async fn conta() -> Value {
    tauri::async_runtime::spawn_blocking(tokens::conta).await.unwrap_or(Value::Null)
}

#[tauri::command]
async fn conexoes_mcp(app: AppHandle) -> Value {
    let r = raiz(&app);
    tauri::async_runtime::spawn_blocking(move || tokens::conexoes_mcp(&r)).await.unwrap_or(Value::Null)
}

/// Retomar uma conversa antiga do Claude Code dentro do programa: liga um id de conversa da tela à sessão.
#[tauri::command]
fn retomar_sessao(app: AppHandle, conversa: String, sessao: String, pasta: String) -> Result<InfoConversa, String> {
    let p = ponte(&app);
    let c = arquivos::resolver(&raiz(&app), &pasta)?;
    p.conversas.ligar_sessao(&conversa, &sessao, &arquivos::exibir(&c))
}

// ---------------------------------------------------------------- bandeja

fn montar_bandeja(app: &AppHandle) -> tauri::Result<()> {
    let p = ponte(app);
    let (mudo, mini) = {
        let c = p.config.lock().unwrap();
        (c.mudo, c.mini_visivel)
    };
    let abrir = MenuItem::with_id(app, "abrir", "Abrir a Ponte de Comando", true, None::<&str>)?;
    let item_mini = MenuItem::with_id(app, "mini", if mini { "Esconder a miniatura" } else { "Mostrar a miniatura" }, true, None::<&str>)?;
    let item_mudo = CheckMenuItem::with_id(app, "mudo", "Silenciar avisos", true, mudo, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let item_sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&abrir, &item_mini, &item_mudo, &sep, &item_sair])?;
    *p.item_mudo.lock().unwrap() = Some(item_mudo);
    *p.item_mini.lock().unwrap() = Some(item_mini);
    TrayIconBuilder::with_id("bandeja")
        .icon(app.default_window_icon().cloned().expect("ícone do programa"))
        .tooltip("Ponte de Comando · Sara")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "abrir" => mostrar_principal(app.clone()),
            "mini" => {
                let vis = ponte(app).config.lock().unwrap().mini_visivel;
                mostrar_mini(app, !vis);
            }
            "mudo" => {
                let m = ponte(app).config.lock().unwrap().mudo;
                aplicar_mudo(app, !m);
            }
            "sair" => sair(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                mostrar_principal(tray.app_handle().clone());
            }
        })
        .build(app)?;
    Ok(())
}

fn sair(app: &AppHandle) {
    let p = ponte(app);
    p.sidecar.encerrar();
    p.conversas.cancelar_todas();
    app.exit(0);
}

// ---------------------------------------------------------------- início

fn pasta_de_dados(app: &AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("PONTE_PASTA_DE_DADOS") {
        return PathBuf::from(p);
    }
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ponte-de-comando"))
}

fn script_do_sidecar(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PONTE_SIDECAR") {
        return Some(PathBuf::from(p));
    }
    // instalado: ao lado dos recursos; em desenvolvimento: a pasta sidecar/ do projeto
    let candidatos = [
        app.path().resource_dir().ok().map(|r| r.join("sidecar").join("ponte-sidecar.mjs")),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("sidecar").join("ponte-sidecar.mjs")),
    ];
    // o Node não abre script com o prefixo \?\ que o Windows põe nos caminhos canônicos
    candidatos.into_iter().flatten().find(|p| p.is_file()).map(|p| PathBuf::from(arquivos::exibir(&p)))
}

pub fn rodar() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            mostrar_principal(app.clone());
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let dados = pasta_de_dados(&handle);
            std::fs::create_dir_all(&dados)?;
            let config = Config::carregar(&dados);
            let token = config::garantir_token(&dados).map_err(|e| Box::<dyn std::error::Error>::from(e))?;
            let sidecar = sidecar::Sidecar::novo(script_do_sidecar(&handle), &dados);
            registrar(&dados, &format!("abrindo; sidecar disponível: {}", sidecar.disponivel()));
            let motor = tokens::Motor::novo();
            app.manage(Ponte {
                pasta_dados: dados.clone(),
                config: Mutex::new(config.clone()),
                estado: Mutex::new(Estado { mudo: config.mudo, ..Default::default() }),
                memoria: Mutex::new(Memoria::default()),
                conversas: Conversas::abrir(&dados),
                sidecar,
                permissoes: Mutex::new(HashMap::new()),
                pedidos: Mutex::new(HashMap::new()),
                inicio: Instant::now(),
                concluido: AtomicBool::new(false),
                arrastando: AtomicBool::new(false),
                ultimo_movimento: Mutex::new(Instant::now()),
                porta: Mutex::new(None),
                item_mudo: Mutex::new(None),
                item_mini: Mutex::new(None),
                motor_tokens: motor.clone(),
            });

            // servidor dos hooks (só 127.0.0.1)
            let h2 = handle.clone();
            match servidor::iniciar(config.porta_efetiva(), token, Arc::new(move |ev| receber_evento(&h2, ev))) {
                Ok(porta) => {
                    *ponte(&handle).porta.lock().unwrap() = Some(porta);
                    let _ = config::gravar_atomico(&dados.join("porta"), porta.to_string().as_bytes());
                    registrar(&dados, &format!("servidor dos hooks em 127.0.0.1:{porta}"));
                }
                Err(e) => registrar(&dados, &format!("servidor dos hooks NÃO abriu na porta {}: {e}", config.porta_efetiva())),
            }

            criar_janelas(&handle)?;
            montar_bandeja(&handle)?;

            // motor de tokens: lê os transcripts em segundo plano e avisa as janelas
            let h3 = handle.clone();
            motor.iniciar(Arc::new(move |resumo| {
                let _ = h3.emit("ponte:tokens", resumo);
            }), Arc::new({
                let h4 = handle.clone();
                move |ev: Evento| {
                    // as sessões abertas pelo programa já avisam pelo "result" do SDK
                    // (e o status pode ficar idle enquanto o cartão de permissão espera)
                    if ev.origem == "sessoes" && ponte(&h4).conversas.tem_sessao(&ev.sessao) {
                        return;
                    }
                    receber_evento(&h4, ev)
                }
            }));

            // se a página de carregamento nunca avisar, segue em 25 s
            let h5 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(25));
                concluir_carregamento(&h5);
            });

            // vigia da miniatura: reafirma "por cima" e devolve para o canto se o monitor ou a barra mudar
            let h6 = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(3));
                let p = ponte(&h6);
                if !p.concluido.load(Ordering::SeqCst) || p.arrastando.load(Ordering::SeqCst) || !p.config.lock().unwrap().mini_visivel {
                    continue;
                }
                if let Some(mini) = h6.get_webview_window("mini") {
                    let _ = mini.set_always_on_top(true);
                    if let (Ok(atual), Some(esperada)) = (mini.outer_position(), posicao_esperada(&h6)) {
                        if (atual.x - esperada.x).abs() > 2 || (atual.y - esperada.y).abs() > 2 {
                            let _ = mini.set_position(esperada);
                        }
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|janela, ev| match (janela.label(), ev) {
            ("principal", WindowEvent::CloseRequested { api, .. }) | ("mini", WindowEvent::CloseRequested { api, .. }) => {
                // fechar só esconde (o programa fica na bandeja)
                api.prevent_close();
                let _ = janela.hide();
            }
            ("carregamento", WindowEvent::Destroyed) => concluir_carregamento(janela.app_handle()),
            ("mini", WindowEvent::ScaleFactorChanged { .. }) => posicionar_mini(janela.app_handle()),
            ("mini", WindowEvent::Moved(_)) => {
                let p = ponte(janela.app_handle());
                if p.arrastando.load(Ordering::SeqCst) {
                    *p.ultimo_movimento.lock().unwrap() = Instant::now();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            estado_atual,
            dispensar_aviso,
            definir_mudo,
            obter_mudo,
            mostrar_principal,
            carregamento_concluido,
            mini_tamanho,
            mini_arrastando,
            mini_soltou,
            mini_canto,
            pasta_de_trabalho,
            listar,
            criar_pasta,
            criar_arquivo,
            ler_arquivo,
            trocar_pasta,
            voltar_padrao,
            enviar,
            parar,
            responder_permissao,
            parar_agente,
            conversa_info,
            info_app,
            tokens_resumo,
            limites,
            conversas_antigas,
            conta,
            conexoes_mcp,
            retomar_sessao
        ])
        .build(tauri::generate_context!())
        .expect("erro ao montar a Ponte de Comando");

    app.run(|app, ev| {
        if let RunEvent::Exit = ev {
            let p = ponte(app);
            p.sidecar.encerrar();
            p.conversas.cancelar_todas();
        }
    });
}
