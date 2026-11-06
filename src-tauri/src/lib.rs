//! Ponte de Comando · Sara — a casca nativa (Tauri 2).
//! Janelas: "carregamento" (abre primeiro, ≥ 8 s), "principal" (Main/Fluxo) e "mini" (Sara no canto).
//! O resto: bandeja, servidor local dos hooks, pasta de trabalho, conversas pelo sidecar do SDK
//! e o motor de tokens (leitura dos transcripts do Claude Code).

mod arquivos;
mod cofre;
mod config;
mod conversa;
mod estado;
mod job;
mod preferencias;
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

const PONTE_JS: &str = include_str!("../../ui/ponte.js");
const MAIN_JS: &str = include_str!("../../ui/ponte-main.js");
/// QA C1: com a página escondida (a casca avisa o WebView2 com IsVisible = false), pausa os vídeos e as
/// animações; volta a tocar só o que ela mesma pausou quando a página aparece de novo.
const PAUSA_JS: &str = r#"(function () {
  if (window.__pontePausa) return
  window.__pontePausa = true
  // a casca também avisa (window.__ponteVisibilidade): o WebView2 escondido para de desenhar, mas nem
  // sempre muda o document.visibilityState; com o aviso, a página inteira (template e adaptador) vê "hidden"
  let escondidoPelaCasca = false
  const D = Document.prototype
  const nHidden = Object.getOwnPropertyDescriptor(D, 'hidden')
  const nVis = Object.getOwnPropertyDescriptor(D, 'visibilityState')
  const escondido = () => escondidoPelaCasca || (nHidden ? nHidden.get.call(document) : false)
  try {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => escondido() })
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => (escondido() ? 'hidden' : (nVis ? nVis.get.call(document) : 'visible')) })
  } catch (e) {}
  window.__ponteVisibilidade = (visivel) => {
    const antes = escondido()
    escondidoPelaCasca = !visivel
    if (escondido() !== antes) document.dispatchEvent(new Event('visibilitychange'))
  }
  const videos = new Set()
  let anims = []
  let vigia = null
  const pausar = () => {
    document.querySelectorAll('video').forEach((v) => { if (!v.paused) { videos.add(v); try { v.pause() } catch (e) {} } })
    try { document.getAnimations().forEach((a) => { if (a.playState === 'running') { anims.push(a); a.pause() } }) } catch (e) {}
  }
  const voltar = () => {
    videos.forEach((v) => { if (v.isConnected) { const r = v.play(); if (r && r.catch) r.catch(() => {}) } })
    videos.clear()
    anims.forEach((a) => { try { if (a.playState === 'paused') a.play() } catch (e) {} })
    anims = []
  }
  // medido em 06/11: a miniatura escondida (janela transparente) seguia desenhando o quadro parado
  // (~27 % de um núcleo) até o <html> ficar com visibility: hidden; com isso cai para ~2 %
  let visAntes = null
  const soMini = /mini\.html$/i.test(location.pathname)
  const aplicar = () => {
    const raiz = document.documentElement
    if (document.hidden) {
      // pausa agora e mais duas vezes (vídeo com autoplay que começa atrasado); sem intervalo fixo, porque
      // document.getAnimations() recalcula o estilo da página inteira e, repetido, custava CPU
      pausar(); clearTimeout(vigia); vigia = setTimeout(() => { pausar(); vigia = setTimeout(pausar, 4000) }, 1500)
      // só na miniatura: na principal o SetIsVisible já basta (0 quadros/s) e o texto da página continua legível
      if (soMini && raiz && visAntes === null) { visAntes = raiz.style.visibility || ''; raiz.style.visibility = 'hidden' }
    } else {
      clearTimeout(vigia); vigia = null
      if (raiz && visAntes !== null) { raiz.style.visibility = visAntes; visAntes = null }
      voltar()
    }
  }
  document.addEventListener('visibilitychange', aplicar)
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', aplicar)
  else aplicar()
})()"#;
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
    item_toast: Mutex<Option<CheckMenuItem<Wry>>>,
    geracao_mini: AtomicU64,
    /// idioma das respostas ou modo da conta mudou: reabrir o sidecar quando nenhuma conversa estiver rodando
    reabrir_sidecar: AtomicBool,
    motor_tokens: Arc<tokens::Motor>,
    /// O que o WebView2 de cada janela sabe sobre estar visível (QA C1).
    visiveis: Mutex<HashMap<String, bool>>,
    /// Avisos fixos da abertura (porta dos hooks ocupada…); a pasta que sumiu é conferida a cada vez.
    alertas_abertura: Mutex<Vec<String>>,
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

/// O programa está rodando da instalação (e não solto em target\debug ou target\release)?
/// Só o instalado tem o atalho com o AppUserModelID "com.pixelmartins.pontedecomando"; o solto
/// mandaria o toast em nome do "Windows PowerShell".
fn instalado() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(dir) = exe.parent() else { return false };
    let d = dir.display().to_string().to_lowercase();
    if d.ends_with("\\target\\debug") || d.ends_with("\\target\\release") {
        return false;
    }
    dir.join("uninstall.exe").is_file()
}

/// Onde o aviso aparece agora.
fn onde_avisar(app: &AppHandle) -> &'static str {
    if principal_na_tela(app) {
        "principal"
    } else if app.get_webview_window("mini").and_then(|m| m.is_visible().ok()).unwrap_or(false) {
        "mini"
    } else {
        "nenhum"
    }
}

/// Os avisos do programa que a tela mostra numa faixa: os da abertura (porta) + a pasta de trabalho
/// gravada que sumiu (QA B1/M5), conferida a cada vez (ela pode sumir com o programa aberto).
fn alertas_atuais(app: &AppHandle) -> Vec<String> {
    let p = ponte(app);
    let mut v = p.alertas_abertura.lock().unwrap().clone();
    let sumiu = p.config.lock().unwrap().pasta_gravada_sumiu();
    if let Some(pasta) = sumiu {
        let padrao = arquivos::exibir(&Config::pasta_padrao());
        v.push(tr(
            app,
            &format!("A pasta de trabalho gravada não existe mais ({pasta}); usando {padrao}. Escolha outra pelo botão PASTA."),
            &format!("The saved work folder no longer exists ({pasta}); using {padrao}. Pick another with the PASTA button."),
        ));
    }
    v
}

fn receber_evento(app: &AppHandle, ev: Evento) {
    let p = ponte(app);
    let (mudo, toast) = {
        let c = p.config.lock().unwrap();
        (c.mudo, c.toast_windows.clone())
    };
    let onde = onde_avisar(app);
    let (copia, notificar) = {
        let mut e = p.estado.lock().unwrap();
        let mut m = p.memoria.lock().unwrap();
        let n = estado::aplicar(&mut e, &mut m, &ev);
        e.mudo = mudo;
        e.onde = onde.to_string();
        (e.clone(), n)
    };
    let mut copia = copia;
    copia.alertas = alertas_atuais(app);
    let _ = app.emit("ponte:estado", &copia);
    if let Some(n) = notificar {
        // a miniatura e a Main já são o aviso; o toast é para quando nenhuma das duas está na tela
        let quer = !mudo && match toast.as_str() {
            "nunca" => false,
            "sempre" => true,
            _ => onde == "nenhum",
        };
        if quer && instalado() {
            if let Err(err) = app.notification().builder().title(&n.titulo).body(&n.corpo).show() {
                registrar(&p.pasta_dados, &format!("notificação falhou: {err}"));
            }
        } else if quer {
            registrar(&p.pasta_dados, "toast não enviado: programa fora da instalação (sairia como Windows PowerShell)");
        }
    }
}

#[tauri::command]
fn estado_atual(app: AppHandle) -> Estado {
    let p = ponte(&app);
    let mut e = p.estado.lock().unwrap().clone();
    e.mudo = p.config.lock().unwrap().mudo;
    e.som = false;
    drop(p);
    e.alertas = alertas_atuais(&app);
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
    if let Some(porta) = config::env_teste("PONTE_DEPURAR_PORTA") {
        if porta.parse::<u16>().is_ok() {
            a.push_str(&format!(" --remote-debugging-port={porta}"));
        }
    }
    a
}

/// Começar na bandeja: aberto pelo Windows (--minimizado) ou "Começar na bandeja" ligado.
fn comecar_na_bandeja(app: &AppHandle) -> bool {
    std::env::args().any(|a| a == "--minimizado") || ponte(app).config.lock().unwrap().comecar_na_bandeja
}

fn criar_janelas(app: &AppHandle) -> tauri::Result<()> {
    let args = argumentos_webview();
    let fora = teste_fora_da_tela();
    let bandeja = comecar_na_bandeja(app);
    let mut carregamento = WebviewWindowBuilder::new(app, "carregamento", WebviewUrl::App("carregamento.html".into()))
        .title("Ponte de Comando")
        .inner_size(960.0, 560.0)
        .resizable(false)
        .decorations(false)
        .center()
        .skip_taskbar(false)
        .initialization_script(PONTE_JS)
        .additional_browser_args(&args)
        .visible(true);
    if fora {
        carregamento = carregamento.position(FORA, FORA).focused(false).focusable(false);
    }
    if !bandeja {
        carregamento.build()?;
    }

    let mut principal = WebviewWindowBuilder::new(app, "principal", WebviewUrl::App("estudio/Main.dc.html".into()))
        .title("Ponte de Comando · Sara")
        .inner_size(1600.0, 900.0)
        .min_inner_size(960.0, 600.0)
        .center()
        .initialization_script(PONTE_JS)
        .initialization_script(MAIN_JS)
        .initialization_script(PAUSA_JS)
        .additional_browser_args(&args)
        .visible(false);
    if fora {
        principal = principal.position(FORA, FORA).focused(false).focusable(false);
    }
    principal.build()?;

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
        .initialization_script(PAUSA_JS)
        .additional_browser_args(&args)
        .visible(false)
        .build()?;
    Ok(())
}

fn mostrar_principal_interno(app: &AppHandle) {
    if let Some(j) = app.get_webview_window("principal") {
        let _ = j.show();
        let _ = j.unminimize();
        if !teste_fora_da_tela() {
            let _ = j.set_focus();
        }
    }
    ajustar_visibilidade(app, false);
    atualizar_mini(app);
}

/// Modo de teste: janelas fora da área visível e sem pegar o foco (o Alex está usando o PC).
fn teste_fora_da_tela() -> bool {
    config::env_teste("PONTE_TESTE_FORA_DA_TELA").map(|v| v == "1").unwrap_or(false)
}
const FORA: f64 = -30000.0;

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
    if teste_fora_da_tela() {
        return Some(PhysicalPosition::new(FORA as i32 + 2000, FORA as i32 + 2000));
    }
    Some(PhysicalPosition::new(x, y))
}

fn posicionar_mini(app: &AppHandle) {
    if let (Some(mini), Some(pos)) = (app.get_webview_window("mini"), posicao_esperada(app)) {
        let _ = mini.set_position(pos);
    }
}

/// A principal está na tela (visível e não minimizada)?
fn principal_na_tela(app: &AppHandle) -> bool {
    app.get_webview_window("principal")
        .map(|j| j.is_visible().unwrap_or(false) && !j.is_minimized().unwrap_or(false))
        .unwrap_or(false)
}

/// QA C1: o `hide()` e o minimizar escondem a JANELA, mas o WebView2 continuava achando que estava na tela
/// (visibilityState "visible", 145 quadros/s, ~3 núcleos com tudo na bandeja). Aqui a casca avisa o WebView2
/// (`ICoreWebView2Controller::SetIsVisible`): escondido, ele para de desenhar, a página recebe
/// `visibilitychange` e o PAUSA_JS para vídeos e animações. Só muda quando o estado muda.
/// `forcar`: reaplica mesmo sem mudança (o vigia de 3 s; cobre o WebView2 que ainda não existia na 1ª
/// chamada e a página que recarregou, como ao ir ao FLUXO e voltar).
fn ajustar_visibilidade(app: &AppHandle, forcar: bool) {
    for rotulo in ["principal", "mini", "carregamento"] {
        let Some(j) = app.get_webview_window(rotulo) else { continue };
        let ver = j.is_visible().unwrap_or(false) && !j.is_minimized().unwrap_or(false);
        {
            let p = ponte(app);
            let mut mapa = p.visiveis.lock().unwrap();
            if mapa.get(rotulo) == Some(&ver) && !forcar {
                continue;
            }
            mapa.insert(rotulo.to_string(), ver);
        }
        let _ = j.eval(&format!("window.__ponteVisibilidade && window.__ponteVisibilidade({ver})"));
        let _ = j.with_webview(move |w| {
            #[cfg(windows)]
            unsafe {
                let _ = w.controller().SetIsVisible(ver);
            }
            #[cfg(not(windows))]
            let _ = (w, ver);
        });
    }
}

/// A regra da miniatura: aparece quando a principal sai da tela (minimizada, na bandeja ou ainda
/// não aberta) e "Miniatura ao minimizar" está ligada; some quando a principal volta.
/// Com debounce de 150 ms, para não piscar ao arrastar ou redimensionar a principal.
fn atualizar_mini(app: &AppHandle) {
    let g = ponte(app).geracao_mini.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        if ponte(&app).geracao_mini.load(Ordering::SeqCst) == g {
            aplicar_mini(&app);
            ajustar_visibilidade(&app, false);
        }
    });
}

fn aplicar_mini(app: &AppHandle) {
    let p = ponte(app);
    if !p.concluido.load(Ordering::SeqCst) || p.arrastando.load(Ordering::SeqCst) {
        return;
    }
    let ligada = p.config.lock().unwrap().mini_visivel;
    let deve = ligada && !principal_na_tela(app);
    let Some(mini) = app.get_webview_window("mini") else { return };
    let visivel = mini.is_visible().unwrap_or(false);
    if deve && !visivel {
        posicionar_mini(app);
        let _ = mini.show();
        let _ = mini.set_always_on_top(true);
    } else if !deve && visivel {
        let _ = mini.hide();
    }
}

fn definir_mini_ao_minimizar(app: &AppHandle, ligada: bool) {
    let p = ponte(app);
    p.config.lock().unwrap().mini_visivel = ligada;
    salvar_config(app);
    let item = p.item_mini.lock().unwrap();
    if let Some(i) = item.as_ref() {
        let _ = i.set_text(if ligada { "Miniatura ao minimizar: ligada" } else { "Miniatura ao minimizar: desligada" });
    }
    drop(item);
    atualizar_mini(app);
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

// ---------------------------------------------------------------- configurações

fn config_publica(app: &AppHandle) -> Value {
    preferencias::publica(&ponte(app).config.lock().unwrap())
}

fn idioma(app: &AppHandle) -> String {
    ponte(app).config.lock().unwrap().idioma.clone()
}

fn tr(app: &AppHandle, pt: &str, en: &str) -> String {
    preferencias::t(&idioma(app), pt, en)
}

/// Iniciar com o Windows (plugin oficial autostart, abre com --minimizado). Nos testes, PONTE_AUTOSTART_FALSO=1.
fn aplicar_autostart(app: &AppHandle, ligar: bool) -> Result<(), String> {
    if config::env_teste("PONTE_AUTOSTART_FALSO").map(|v| v == "1").unwrap_or(false) {
        return Ok(());
    }
    let a = app.autolaunch();
    let r = if ligar { a.enable() } else { a.disable() };
    r.map_err(|e| format!("{} {e}", tr(app, "O Windows não deixou mudar o início automático:", "Windows refused to change start-up:")))
}

#[tauri::command]
fn config_ler(app: AppHandle) -> Value {
    config_publica(&app)
}

/// Grava só as chaves conhecidas, validando o pedido inteiro antes (preferencias::aplicar),
/// aplica os efeitos e avisa todas as janelas (`ponte:config`).
#[tauri::command]
fn config_gravar(app: AppHandle, parcial: Value) -> Result<Value, String> {
    let p = ponte(&app);
    let atual = p.config.lock().unwrap().clone();
    if parcial.get("modoConta").and_then(|v| v.as_str()) == Some("api") && !cofre::existe(&cofre::alvo()) {
        return Err(tr(&app, "Guarde uma chave de API antes de usar o modo chave.", "Store an API key before switching to API-key mode."));
    }
    let (novo, ef) = preferencias::aplicar(&atual, &parcial)?;
    if let Some(b) = ef.autostart {
        aplicar_autostart(&app, b)?;
    }
    *p.config.lock().unwrap() = novo;
    salvar_config(&app);
    if ef.mini {
        let ligada = p.config.lock().unwrap().mini_visivel;
        definir_mini_ao_minimizar(&app, ligada);
    }
    if let Some(m) = ef.mudo {
        aplicar_mudo(&app, m);
    }
    if ef.canto {
        posicionar_mini(&app);
        let canto = p.config.lock().unwrap().mini_canto.clone();
        let _ = app.emit("ponte:mini-canto", &canto);
    }
    {
        let sempre = p.config.lock().unwrap().toast_windows == "sempre";
        let item = p.item_toast.lock().unwrap();
        if let Some(i) = item.as_ref() {
            let _ = i.set_checked(sempre);
        }
    }
    if ef.pasta {
        let _ = app.emit("ponte:pasta", info_pasta(&app));
    }
    if ef.reabrir_sidecar {
        p.reabrir_sidecar.store(true, Ordering::SeqCst);
    }
    let atual = config_publica(&app);
    let _ = app.emit("ponte:config", &atual);
    Ok(atual)
}

// ---------------------------------------------------------------- conta (assinatura ou chave de API)

fn status_da_conta(app: &AppHandle) -> Value {
    let mut v = tokens::conta();
    if let Some(e) = v.get("email").and_then(|x| x.as_str()).map(String::from) {
        v["email"] = json!(cofre::mascarar_email(&e));
    }
    let modo = ponte(app).config.lock().unwrap().modo_conta.clone();
    v["modo"] = json!(modo);
    v["temChave"] = json!(cofre::existe(&cofre::alvo()));
    v
}

#[tauri::command]
async fn conta_status(app: AppHandle) -> Value {
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || status_da_conta(&a)).await.unwrap_or(Value::Null)
}

#[tauri::command]
fn conta_modo(app: AppHandle) -> String {
    ponte(&app).config.lock().unwrap().modo_conta.clone()
}

/// Abre o login OFICIAL do Claude Code num terminal visível (o OAuth acontece no navegador, à vista).
/// Quando a janela fecha, o status é relido e vai para as janelas em `ponte:conta`.
#[tauri::command]
fn conta_trocar(app: AppHandle) -> Result<(), String> {
    let exe = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(".local").join("bin").join("claude.exe");
    if !exe.is_file() {
        return Err(tr(&app, "Não achei o claude.exe para abrir o login.", "Could not find claude.exe to start the login."));
    }
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(["auth", "login"]).env_remove("CLAUDECODE").env_remove("ANTHROPIC_API_KEY");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0010); // CREATE_NEW_CONSOLE: o login fica à vista
    }
    let mut filho = cmd.spawn().map_err(|e| e.to_string())?;
    registrar(&ponte(&app).pasta_dados, "login do Claude Code aberto num terminal");
    std::thread::spawn(move || {
        let _ = filho.wait();
        let st = status_da_conta(&app);
        let _ = app.emit("ponte:conta", &st);
    });
    Ok(())
}

/// Logout oficial. A tela confirma antes e manda `confirmado: true`.
#[tauri::command]
async fn conta_sair(app: AppHandle, confirmado: bool) -> Result<Value, String> {
    if !confirmado {
        return Err(tr(&app, "Confirme na tela antes de sair da conta.", "Confirm on screen before signing out."));
    }
    let a = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        let r = tokens::rodar_claude_publico(&["auth", "logout"], None, Duration::from_secs(30));
        (r, status_da_conta(&a))
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = app.emit("ponte:conta", &r.1);
    r.0.map(|_| r.1)
}

#[tauri::command]
fn conta_guardar_chave(app: AppHandle, chave: String) -> Result<(), String> {
    let chave = chave.trim().to_string();
    if !cofre::formato_valido(&chave) {
        return Err(tr(&app, "Isso não parece uma chave da Anthropic (começa com sk-ant-).", "That does not look like an Anthropic key (it starts with sk-ant-)."));
    }
    cofre::guardar(&cofre::alvo(), &chave)?;
    registrar(&ponte(&app).pasta_dados, "chave de API guardada no Cofre do Windows"); // nunca a chave
    if ponte(&app).config.lock().unwrap().modo_conta == "api" {
        ponte(&app).reabrir_sidecar.store(true, Ordering::SeqCst);
    }
    let _ = app.emit("ponte:conta", json!({ "temChave": true }));
    Ok(())
}

/// Chamada mínima e gratuita (lista de modelos) com a chave do Cofre. Diz OK ou o erro, nunca a chave.
#[tauri::command]
async fn conta_testar_chave(app: AppHandle, chave: Option<String>) -> Value {
    // com a chave digitada na tela (ainda não guardada), testa ela; sem, testa a do Cofre. Nunca grava nada.
    let digitada = chave.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    if let Some(c) = &digitada {
        if !cofre::formato_valido(c) {
            return json!({ "ok": false, "erro": tr(&app, "Isso não parece uma chave da Anthropic (começa com sk-ant-).", "That does not look like an Anthropic key (it starts with sk-ant-).") });
        }
    }
    let Some(chave) = digitada.or_else(|| cofre::ler(&cofre::alvo())) else {
        return json!({ "ok": false, "erro": tr(&app, "Não há chave guardada.", "No key stored.") });
    };
    let p = ponte(&app);
    let (Some(node), Some(script)) = (p.sidecar.node.clone(), p.sidecar.script.clone()) else {
        return json!({ "ok": false, "erro": tr(&app, "Sem o Node não dá para testar.", "Node is required to test the key.") });
    };
    let testar = script.with_file_name("testar-chave.mjs");
    let en = idioma(&app) == "en";
    tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(node);
        cmd.arg(&testar).env("ANTHROPIC_API_KEY", &chave).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let saida = cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn().and_then(|f| {
            crate::job::amarrar(f.id()); // fecha junto com a Ponte
            f.wait_with_output()
        });
        drop(chave);
        match saida {
            Ok(o) => serde_json::from_slice::<Value>(&o.stdout).unwrap_or_else(|_| json!({ "ok": false, "erro": if en { "unexpected answer from the test" } else { "resposta inesperada do teste" } })),
            Err(e) => json!({ "ok": false, "erro": e.to_string() }),
        }
    })
    .await
    .unwrap_or(Value::Null)
}

#[tauri::command]
fn conta_apagar_chave(app: AppHandle) -> bool {
    let apagou = cofre::apagar(&cofre::alvo());
    let p = ponte(&app);
    // sem chave não há modo chave
    if p.config.lock().unwrap().modo_conta == "api" {
        p.config.lock().unwrap().modo_conta = "assinatura".into();
        salvar_config(&app);
        p.reabrir_sidecar.store(true, Ordering::SeqCst);
        let _ = app.emit("ponte:config", config_publica(&app));
    }
    registrar(&p.pasta_dados, "chave de API apagada do Cofre");
    let _ = app.emit("ponte:conta", json!({ "temChave": false }));
    apagou
}

// ---------------------------------------------------------------- sistema

/// Abre no Explorer a pasta de dados do programa (caminho fixo; não aceita caminho de fora).
#[tauri::command]
fn sistema_abrir_pasta_dados(app: AppHandle) -> Result<(), String> {
    let pasta = ponte(&app).pasta_dados.clone();
    std::process::Command::new(r"C:\Windows\explorer.exe").arg(&pasta).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn sistema_exportar_config(app: AppHandle) -> Result<Option<String>, String> {
    let conteudo = serde_json::to_string_pretty(&preferencias::exportavel(&ponte(&app).config.lock().unwrap())).map_err(|e| e.to_string())?;
    let a = app.clone();
    let titulo = tr(&app, "Salvar as configurações", "Save settings");
    let escolha = tauri::async_runtime::spawn_blocking(move || {
        let mut d = a.dialog().file().set_title(titulo).set_file_name("ponte-de-comando-config.json").add_filter("JSON", &["json"]);
        if let Some(j) = a.get_webview_window("principal") {
            d = d.set_parent(&j);
        }
        d.blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(arq) = escolha else { return Ok(None) };
    let caminho = arq.into_path().map_err(|e| e.to_string())?;
    config::gravar_atomico(&caminho, conteudo.as_bytes())?;
    Ok(Some(arquivos::exibir(&caminho)))
}

/// Lê um arquivo exportado, valida TUDO e só então aplica (o mesmo caminho do config_gravar).
#[tauri::command]
async fn sistema_importar_config(app: AppHandle) -> Result<Option<Value>, String> {
    let a = app.clone();
    let titulo = tr(&app, "Abrir configurações", "Open settings");
    let escolha = tauri::async_runtime::spawn_blocking(move || {
        let mut d = a.dialog().file().set_title(titulo).add_filter("JSON", &["json"]);
        if let Some(j) = a.get_webview_window("principal") {
            d = d.set_parent(&j);
        }
        d.blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(arq) = escolha else { return Ok(None) };
    let caminho = arq.into_path().map_err(|e| e.to_string())?;
    importar_de(&app, &caminho).map(Some)
}

fn importar_de(app: &AppHandle, caminho: &std::path::Path) -> Result<Value, String> {
    let meta = std::fs::metadata(caminho).map_err(|e| e.to_string())?;
    if meta.len() > 64 * 1024 {
        return Err(tr(app, "Arquivo grande demais para ser uma configuração.", "File too large to be a settings file."));
    }
    let texto = std::fs::read_to_string(caminho).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&texto).map_err(|_| tr(app, "O arquivo não é um JSON válido.", "The file is not valid JSON."))?;
    if v.get("programa").and_then(|x| x.as_str()) != Some("Ponte de Comando") {
        return Err(tr(app, "Esse arquivo não é uma configuração da Ponte de Comando.", "This file is not a Ponte de Comando settings file."));
    }
    let cfg = v.get("config").cloned().ok_or_else(|| tr(app, "O arquivo não tem a parte \"config\".", "The file has no \"config\" section."))?;
    config_gravar(app.clone(), cfg)
}

/// Teste: importar sem o seletor (só com PONTE_TESTE_FORA_DA_TELA=1).
#[tauri::command]
fn sistema_importar_arquivo_teste(app: AppHandle, caminho: String) -> Result<Value, String> {
    if !cfg!(debug_assertions) || !teste_fora_da_tela() {
        return Err("só nos testes".into());
    }
    importar_de(&app, std::path::Path::new(&caminho))
}

/// Volta tudo que a tela de CONFIGURAÇÕES mexe ao padrão (com confirmação na tela).
#[tauri::command]
fn sistema_restaurar_padrao(app: AppHandle, confirmado: bool) -> Result<Value, String> {
    if !confirmado {
        return Err(tr(&app, "Confirme na tela antes de restaurar o padrão.", "Confirm on screen before restoring defaults."));
    }
    let p = ponte(&app);
    let novo = preferencias::padrao_mantendo(&p.config.lock().unwrap());
    let _ = aplicar_autostart(&app, false);
    *p.config.lock().unwrap() = novo;
    salvar_config(&app);
    definir_mini_ao_minimizar(&app, true);
    aplicar_mudo(&app, false);
    posicionar_mini(&app);
    p.reabrir_sidecar.store(true, Ordering::SeqCst);
    let _ = app.emit("ponte:pasta", info_pasta(&app));
    let atual = config_publica(&app);
    let _ = app.emit("ponte:config", &atual);
    Ok(atual)
}

// ---------------------------------------------------------------- pasta de trabalho

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InfoPasta {
    caminho: String,
    nome: String,
    padrao: bool,
    caminho_padrao: String,
    /// A pasta gravada que sumiu (o programa voltou para a padrão e avisa).
    aviso: Option<String>,
}

fn info_pasta(app: &AppHandle) -> InfoPasta {
    let (p, padrao) = ponte(app).config.lock().unwrap().pasta_atual();
    InfoPasta {
        caminho: arquivos::exibir(&p),
        nome: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| arquivos::exibir(&p)),
        padrao,
        caminho_padrao: arquivos::exibir(&Config::pasta_padrao()),
        aviso: ponte(app).config.lock().unwrap().pasta_gravada_sumiu().map(|p| format!("A pasta de trabalho gravada não existe mais ({p}); usando {}.", arquivos::exibir(&Config::pasta_padrao()))),
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
    if let Some(e) = pasta_proibida(&c) {
        return Err(e);
    }
    let texto = arquivos::exibir(&c);
    ponte(&app).config.lock().unwrap().pasta_de_trabalho = Some(texto.clone());
    salvar_config(&app);
    let _ = app.emit("ponte:pasta", info_pasta(&app));
    let _ = app.emit("ponte:config", config_publica(&app));
    Ok(Some(texto))
}

/// Pastas que não podem virar a pasta de trabalho: raiz de disco e a pasta do Windows.
fn pasta_proibida(c: &std::path::Path) -> Option<String> {
    let texto = arquivos::exibir(c).to_lowercase();
    if c.parent().is_none() || texto.trim_end_matches('\\').len() <= 2 {
        return Some("A raiz de um disco não pode ser a pasta de trabalho. Escolha uma pasta dentro dele.".into());
    }
    let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    if texto == windows || texto.starts_with(&format!("{windows}\\")) {
        return Some("A pasta do Windows não pode ser a pasta de trabalho.".into());
    }
    None
}

#[tauri::command]
fn voltar_padrao(app: AppHandle) -> InfoPasta {
    ponte(&app).config.lock().unwrap().pasta_de_trabalho = None;
    salvar_config(&app);
    let i = info_pasta(&app);
    let _ = app.emit("ponte:pasta", &i);
    let _ = app.emit("ponte:config", config_publica(&app));
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
            let info = p.conversas.info(&conversa);
            let projeto = info.as_ref().map(|i| PathBuf::from(&i.pasta).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).unwrap_or_default();
            // a sessão do Claude Code desta conversa: o aviso dela não é apagado por outra (QA M3)
            let sessao = info.map(|i| i.sessao).unwrap_or_default();
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
                    receber_evento(app, Evento { tipo: "trabalhando".into(), fase: "ferramenta".into(), sessao, projeto, ferramenta: nome, legenda: leg, origem: "programa".into(), ..Default::default() });
                }
                // QA A1: o turno que o Alex PAROU chega como resultado "cancelado": não é erro nem pronto
                "resultado" if v.get("cancelado").and_then(|x| x.as_bool()).unwrap_or(false) => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    p.motor_tokens.cutucar();
                }
                "resultado" => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    let erro = v.get("erro").and_then(|x| x.as_bool()).unwrap_or(false);
                    let texto = v.get("texto").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let titulo = p.pedidos.lock().unwrap().get(&conversa).cloned().unwrap_or_default();
                    receber_evento(app, Evento { tipo: if erro { "erro".into() } else { "pronto".into() }, sessao, projeto, titulo: if erro { String::new() } else { titulo }, texto: if erro { texto.clone() } else { String::new() }, resumo: texto, origem: "programa".into(), ..Default::default() });
                    p.motor_tokens.cutucar();
                }
                "erro" => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    let texto = v.get("texto").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    receber_evento(app, Evento { tipo: "erro".into(), sessao, projeto, texto, origem: "programa".into(), ..Default::default() });
                }
                "cancelado" => {
                    p.conversas.marcar_rodando_sdk(&conversa, false);
                    receber_evento(app, Evento { tipo: "esperando".into(), sessao, origem: "programa".into(), ..Default::default() });
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
            let sessao = p.conversas.info(&conversa).map(|i| i.sessao).unwrap_or_default();
            let _ = app.emit("ponte:permissao", json!({ "conversa": conversa, "pedido": pedido, "ferramenta": ferramenta, "frase": frase, "detalhe": detalhe }));
            receber_evento(app, Evento { tipo: "precisa".into(), sessao, texto: format!("Pedido de permissão: {frase}"), origem: "programa".into(), ..Default::default() });
        }
        "permissao_fim" => {
            let pedido = v.get("pedido").and_then(|x| x.as_str()).unwrap_or("").to_string();
            p.permissoes.lock().unwrap().remove(&pedido);
            let _ = app.emit("ponte:permissao-fim", json!({ "conversa": conversa, "pedido": pedido }));
        }
        "sidecar_caiu" => {
            registrar(&p.pasta_dados, "o sidecar saiu");
            let texto = tr(app, "O processo da conversa fechou. A próxima mensagem retoma a mesma conversa.", "The conversation process closed. The next message resumes the same conversation.");
            let rodando = p.conversas.limpar_rodando_sdk();
            for c in &rodando {
                emitir_conversa(app, &json!({ "evento": "conversa", "conversa": c, "tipo": "erro", "texto": texto }));
            }
            let pend: Vec<(String, String)> = p.permissoes.lock().unwrap().drain().collect();
            let tinha_cartao = !pend.is_empty();
            for (pedido, c) in pend {
                let _ = app.emit("ponte:permissao-fim", json!({ "conversa": c, "pedido": pedido }));
            }
            // QA M1: a Sara não pode ficar presa em "preciso de você" (ou "trabalhando") com a conversa morta
            if !rodando.is_empty() || tinha_cartao {
                let sessao = rodando.first().and_then(|c| p.conversas.info(c)).map(|i| i.sessao).unwrap_or_default();
                receber_evento(app, Evento { tipo: "erro".into(), sessao, texto, origem: "programa".into(), ..Default::default() });
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
    // conversa sem modelo/esforço/velocidade na aba: usa o padrão da conversa nova (ou o que ela já tinha)
    let (cfg_padrao, instrucao, lembrar, modo_api) = {
        let c = p.config.lock().unwrap();
        (c.padrao_nova.clone(), (preferencias::instrucao_idioma(&c), preferencias::language_do_claude(&c), preferencias::marca_idioma(&c)), c.lembrar_aprovacoes, c.modo_conta == "api")
    };
    let mut pedido = pedido;
    if pedido.modelo.trim().is_empty() {
        pedido.modelo = ja.as_ref().map(|i| i.modelo.clone()).unwrap_or(cfg_padrao.modelo);
    }
    if pedido.esforco.trim().is_empty() {
        pedido.esforco = ja.as_ref().map(|i| i.esforco.clone()).unwrap_or(cfg_padrao.esforco);
    }
    if pedido.rapido.is_none() && pedido.velocidade.trim().is_empty() {
        pedido.rapido = Some(ja.as_ref().map(|i| i.rapido).unwrap_or(cfg_padrao.rapido));
    }
    let info = p.conversas.preparar(&pedido, pasta)?;
    // modo da conta: a chave do Cofre (ou nenhuma) vale para o próximo sidecar; trocar pede reabrir
    let chave = if modo_api { cofre::ler(&cofre::alvo()) } else { None };
    if modo_api && chave.is_none() {
        return Err(tr(&app, "O modo chave de API está ligado, mas não há chave no Cofre.", "API-key mode is on, but there is no key in the vault."));
    }
    if p.sidecar.definir_chave(chave) {
        p.reabrir_sidecar.store(true, Ordering::SeqCst);
    }
    if p.reabrir_sidecar.load(Ordering::SeqCst) && p.conversas.nenhuma_rodando() {
        p.reabrir_sidecar.store(false, Ordering::SeqCst);
        if p.sidecar.vivo() {
            registrar(&p.pasta_dados, "reabrindo o sidecar (idioma ou conta mudou)");
            p.sidecar.encerrar();
        }
    }
    let _ = lembrar;
    if !PathBuf::from(&info.pasta).is_dir() {
        return Err(format!("A pasta desta conversa não existe mais: {}", info.pasta));
    }
    p.conversas.marcar_rodando_sdk(&pedido.conversa, true);
    let projeto = PathBuf::from(&info.pasta).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    receber_evento(&app, Evento { tipo: "trabalhando".into(), fase: "pedido".into(), sessao: info.sessao.clone(), projeto, titulo: estado::cortar(&texto, 140), origem: "programa".into(), ..Default::default() });
    let cmd = json!({
        "cmd": "enviar", "conversa": pedido.conversa, "texto": texto,
        "modelo": info.modelo, "esforco": info.esforco, "rapido": info.rapido,
        "cwd": info.pasta, "sessao": info.sessao, "retomar": info.iniciada,
        "instrucao": instrucao.0, "language": instrucao.1, "marcaIdioma": instrucao.2
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
    let lembrar = p.config.lock().unwrap().lembrar_aprovacoes;
    let ok = p.sidecar.mandar_se_vivo(&json!({ "cmd": "permissao", "pedido": pedido, "decisao": if permitir { "permitir" } else { "negar" }, "motivo": motivo, "lembrar": lembrar && permitir }));
    registrar(&p.pasta_dados, &format!("permissão {pedido}: {}", if permitir { "permitida" } else { "negada" }));
    let _ = app.emit("ponte:permissao-fim", json!({ "pedido": pedido }));
    dispensar_aviso(app.clone());
    if ok { Ok(()) } else { Err("A conversa não está mais aberta.".into()) }
}

#[tauri::command]
fn parar_agente(app: AppHandle, conversa: String, tarefa: String) -> bool {
    ponte(&app).sidecar.mandar_se_vivo(&json!({ "cmd": "parar_agente", "conversa": conversa, "tarefa": tarefa }))
}

/// A conversa e se ela está trabalhando agora (a Main, ao recarregar, volta a mostrar o PARAR).
#[tauri::command]
fn conversa_info(app: AppHandle, conversa: String) -> Option<Value> {
    let p = ponte(&app);
    let info = p.conversas.info(&conversa)?;
    let mut v = serde_json::to_value(info).ok()?;
    v["rodando"] = json!(p.conversas.rodando(&conversa));
    Some(v)
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
        "toast": { "instalado": instalado(), "modo": p.config.lock().unwrap().toast_windows.clone() },
        "miniAoMinimizar": p.config.lock().unwrap().mini_visivel,
        "janelas": {
            "carregamento": app.get_webview_window("carregamento").and_then(|j| j.is_visible().ok()),
            "principal": app.get_webview_window("principal").and_then(|j| j.is_visible().ok()),
            "principalMinimizada": app.get_webview_window("principal").and_then(|j| j.is_minimized().ok()),
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
fn conversas_antigas(app: AppHandle, pasta: Option<String>, limite: Option<usize>) -> Result<Value, String> {
    // só pastas dentro da pasta de trabalho (o resolver recusa caminho de fora)
    let alvo = arquivos::resolver(&raiz(&app), pasta.as_deref().unwrap_or(""))?;
    Ok(tokens::conversas_antigas(&alvo, limite.unwrap_or(50).min(200)))
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
    let (mudo, mini, sempre) = {
        let c = p.config.lock().unwrap();
        (c.mudo, c.mini_visivel, c.toast_windows == "sempre")
    };
    let abrir = MenuItem::with_id(app, "abrir", "Abrir a Ponte de Comando", true, None::<&str>)?;
    let item_mini = MenuItem::with_id(app, "mini", if mini { "Miniatura ao minimizar: ligada" } else { "Miniatura ao minimizar: desligada" }, true, None::<&str>)?;
    let item_toast = CheckMenuItem::with_id(app, "toast", "Notificação do Windows sempre", true, sempre, None::<&str>)?;
    let item_mudo = CheckMenuItem::with_id(app, "mudo", "Silenciar avisos", true, mudo, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let item_sair = MenuItem::with_id(app, "sair", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&abrir, &item_mini, &item_mudo, &item_toast, &sep, &item_sair])?;
    *p.item_mudo.lock().unwrap() = Some(item_mudo);
    *p.item_mini.lock().unwrap() = Some(item_mini);
    *p.item_toast.lock().unwrap() = Some(item_toast);
    TrayIconBuilder::with_id("bandeja")
        .icon(app.default_window_icon().cloned().expect("ícone do programa"))
        .tooltip("Ponte de Comando · Sara")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "abrir" => mostrar_principal(app.clone()),
            "mini" => {
                let vis = ponte(app).config.lock().unwrap().mini_visivel;
                definir_mini_ao_minimizar(app, !vis);
            }
            "toast" => {
                let sempre = ponte(app).config.lock().unwrap().toast_windows == "sempre";
                let _ = config_gravar(app.clone(), json!({ "toastWindows": if sempre { "auto" } else { "sempre" } }));
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
    if let Some(p) = config::env_teste("PONTE_PASTA_DE_DADOS") {
        return PathBuf::from(p);
    }
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ponte-de-comando"))
}

fn script_do_sidecar(app: &AppHandle) -> Option<PathBuf> {
    if let Some(p) = config::env_teste("PONTE_SIDECAR") {
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

/// QA B11: a instância única usa o `identifier`. Nos testes (build de desenvolvimento com
/// PONTE_PASTA_DE_DADOS), cada pasta de dados vira um identificador próprio: o teste nunca "acha" a Ponte
/// instalada do Alex (nem põe a janela dele na frente), e duas aberturas com a MESMA pasta continuam sendo
/// uma instância só.
fn identificador_de_teste(base: &str) -> Option<String> {
    let pasta = config::env_teste("PONTE_PASTA_DE_DADOS")?;
    // FNV-1a de 32 bits do caminho (sem caixa): curto, estável e sem dependência nova
    let mut h: u32 = 0x811c_9dc5;
    for b in pasta.to_lowercase().bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    Some(format!("{base}.teste{h:08x}"))
}

pub fn rodar() {
    let mut contexto = tauri::generate_context!();
    if let Some(id) = identificador_de_teste(&contexto.config().identifier) {
        contexto.config_mut().identifier = id;
    }
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            mostrar_principal(app.clone());
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimizado"])))
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
                item_toast: Mutex::new(None),
                geracao_mini: AtomicU64::new(0),
                reabrir_sidecar: AtomicBool::new(false),
                motor_tokens: motor.clone(),
                visiveis: Mutex::new(HashMap::new()),
                alertas_abertura: Mutex::new(vec![]),
            });

            // servidor dos hooks (só 127.0.0.1)
            // servidor dos hooks: id novo a cada execução, conferido pelo hook antes de mandar o token
            let h2 = handle.clone();
            let id_servidor = config::aleatorio_hex(16);
            let pedida = config.porta_efetiva();
            let (aberta, falhas) = servidor::iniciar(pedida, token, id_servidor.clone(), Arc::new(move |ev| receber_evento(&h2, ev)));
            for (p, e) in &falhas {
                registrar(&dados, &format!("servidor dos hooks NÃO abriu na porta {p}: {e}"));
            }
            let il = config.idioma.clone();
            let tx = |pt: String, en: String| preferencias::t(&il, &pt, &en);
            let mut alertas = vec![];
            match aberta {
                Some(porta) => {
                    *ponte(&handle).porta.lock().unwrap() = Some(porta);
                    let _ = config::gravar_atomico(&dados.join("porta"), format!("{porta} {id_servidor}").as_bytes());
                    registrar(&dados, &format!("servidor dos hooks em 127.0.0.1:{porta}"));
                    if porta != pedida {
                        alertas.push(tx(
                            format!("A porta {pedida} dos hooks estava ocupada; o servidor dos hooks abriu na porta {porta} (o hook acompanha sozinho)."),
                            format!("Hook port {pedida} was busy; the hook server opened on port {porta} (the hook follows it)."),
                        ));
                    }
                }
                None => {
                    let _ = std::fs::remove_file(dados.join("porta"));
                    alertas.push(tx(
                        format!("O servidor dos hooks não abriu (portas {pedida} a {} ocupadas): os avisos de conversas de fora não vão chegar.", pedida.saturating_add(10)),
                        format!("The hook server did not open (ports {pedida} to {} busy): notices from outside conversations will not arrive.", pedida.saturating_add(10)),
                    ));
                }
            }
            *ponte(&handle).alertas_abertura.lock().unwrap() = alertas;
            let al = alertas_atuais(&handle);
            ponte(&handle).estado.lock().unwrap().alertas = al;

            criar_janelas(&handle)?;
            montar_bandeja(&handle)?;
            if comecar_na_bandeja(&handle) {
                // sem carregamento e sem principal: só a Sara no canto (se "Miniatura ao minimizar" estiver ligada)
                ponte(&handle).concluido.store(true, Ordering::SeqCst);
                atualizar_mini(&handle);
                registrar(&dados, "começou na bandeja");
            }

            // motor de tokens: lê os transcripts em segundo plano e avisa as janelas
            let h3 = handle.clone();
            motor.iniciar(Arc::new(move |resumo| {
                // só a Main mostra o consumo: escondida, ela não trabalha (relê tokens() quando volta)
                if principal_na_tela(&h3) {
                    let _ = h3.emit_to("principal", "ponte:tokens", resumo);
                }
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
            let mut voltas: u64 = 0;
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(3));
                // reaplica à força nos primeiros 30 s (o WebView2 pode nem existir na 1ª chamada) e depois a
                // cada 30 s; no resto, só quando muda (acordar as páginas a cada 3 s custava CPU)
                voltas += 1;
                ajustar_visibilidade(&h6, voltas <= 10 || voltas % 10 == 0);
                let p = ponte(&h6);
                if !p.concluido.load(Ordering::SeqCst) || p.arrastando.load(Ordering::SeqCst) {
                    continue;
                }
                // confere a regra de novo, para nunca ficar presa num estado errado
                aplicar_mini(&h6);
                if !h6.get_webview_window("mini").and_then(|m| m.is_visible().ok()).unwrap_or(false) {
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
                atualizar_mini(janela.app_handle());
            }
            ("principal", WindowEvent::Resized(_)) => {
                // minimizar e restaurar chegam aqui: a página sabe na hora (sem esperar o debounce)
                ajustar_visibilidade(janela.app_handle(), false);
                atualizar_mini(janela.app_handle());
            }
            ("principal", WindowEvent::Moved(_)) => atualizar_mini(janela.app_handle()),
            ("principal", WindowEvent::Focused(foco)) => {
                ajustar_visibilidade(janela.app_handle(), false);
                atualizar_mini(janela.app_handle());
                // voltou para a Main: o aviso de pronto que esperava já foi visto
                if *foco {
                    let velho = ponte(janela.app_handle()).estado.lock().unwrap().aviso.as_ref().map(|a| a.tipo == "pronto" && config::agora_ms().saturating_sub(a.em) > 2000).unwrap_or(false);
                    if velho {
                        dispensar_aviso(janela.app_handle().clone());
                    }
                }
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
            config_ler,
            config_gravar,
            conta_status,
            conta_modo,
            conta_trocar,
            conta_sair,
            conta_guardar_chave,
            conta_testar_chave,
            conta_apagar_chave,
            sistema_abrir_pasta_dados,
            sistema_exportar_config,
            sistema_importar_config,
            sistema_importar_arquivo_teste,
            sistema_restaurar_padrao,
            dispensar_aviso,
            definir_mudo,
            obter_mudo,
            mostrar_principal,
            carregamento_concluido,
            mini_tamanho,
            mini_arrastando,
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
        .build(contexto)
        .expect("erro ao montar a Ponte de Comando");

    app.run(|app, ev| {
        if let RunEvent::Exit = ev {
            let p = ponte(app);
            p.sidecar.encerrar();
            p.conversas.cancelar_todas();
        }
    });
}
