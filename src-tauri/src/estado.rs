//! O estado que a miniatura (e a placa da Main) mostra, e como cada evento muda ele.
//! Eventos chegam do hook do Claude Code (servidor local) e das conversas abertas pelo programa.

use crate::config::agora_ms;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct Evento {
    /// "trabalhando" | "precisa" | "pronto" | "erro" | "esperando"
    pub tipo: String,
    /// "pedido" | "ferramenta" | "ferramenta-fim" | "subagente" (só para "trabalhando")
    pub fase: String,
    pub sessao: String,
    pub projeto: String,
    /// Título do pedido (UserPromptSubmit) ou pedido lembrado pelo hook (Stop).
    pub titulo: String,
    pub ferramenta: String,
    /// Linha curta já pronta para mostrar ("Lendo LEIA-ME.md").
    pub legenda: String,
    /// Texto livre: mensagem da Notification, erro…
    pub texto: String,
    /// Resumo do que foi feito (Stop: última fala do Claude, se o hook conseguiu).
    pub resumo: String,
    /// "hook" ou "programa"
    pub origem: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Aviso {
    /// "pronto" | "precisa" | "erro"
    pub tipo: String,
    pub topo: String,
    pub titulo: String,
    pub texto: String,
    pub acao: String,
    pub em: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Estado {
    /// "esperando" | "trabalhando" | "precisa" | "pronto" | "erro"
    pub estado: String,
    pub legenda: String,
    pub pedido: String,
    pub projeto: String,
    pub ferramenta: String,
    pub aviso: Option<Aviso>,
    /// true = este evento pede som (pronto/precisa/erro) — a miniatura toca se não estiver mudo.
    pub som: bool,
    pub mudo: bool,
    /// Onde o aviso aparece agora: "principal" (a Main está na tela), "mini" ou "nenhum" (vira toast).
    pub onde: String,
    pub em: u64,
}

impl Default for Estado {
    fn default() -> Self {
        Estado {
            estado: "esperando".into(),
            legenda: "Por aqui".into(),
            pedido: String::new(),
            projeto: String::new(),
            ferramenta: String::new(),
            aviso: None,
            som: false,
            mudo: false,
            onde: String::new(),
            em: agora_ms(),
        }
    }
}

/// O que o programa deve fazer além de repassar o estado.
#[derive(Debug, PartialEq)]
pub struct Notificar {
    pub titulo: String,
    pub corpo: String,
}

#[derive(Default)]
pub struct Memoria {
    /// Último pedido de cada sessão do Claude Code (para o resumo do "pronto").
    pedidos: HashMap<String, (String, String)>,
}

pub fn cortar(s: &str, max: usize) -> String {
    let limpo: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let limpo = limpo.split_whitespace().collect::<Vec<_>>().join(" ");
    if limpo.chars().count() <= max {
        limpo
    } else {
        let mut t: String = limpo.chars().take(max.saturating_sub(1)).collect();
        t = t.trim_end().to_string();
        t.push('…');
        t
    }
}

impl Memoria {
    fn lembrar(&mut self, sessao: &str, pedido: &str, projeto: &str) {
        if self.pedidos.len() > 50 {
            self.pedidos.clear();
        }
        self.pedidos.insert(sessao.to_string(), (pedido.to_string(), projeto.to_string()));
    }
    fn pedido(&self, sessao: &str) -> Option<&(String, String)> {
        self.pedidos.get(sessao)
    }
}

/// Aplica um evento. Devolve `Some(Notificar)` quando deve sair notificação do Windows.
pub fn aplicar(atual: &mut Estado, mem: &mut Memoria, ev: &Evento) -> Option<Notificar> {
    let agora = agora_ms();
    let projeto = cortar(&ev.projeto, 60);
    atual.som = false;
    atual.em = agora;
    match ev.tipo.as_str() {
        "trabalhando" => {
            if ev.fase == "pedido" {
                let titulo = cortar(&ev.titulo, 140);
                if !ev.sessao.is_empty() && !titulo.is_empty() {
                    mem.lembrar(&ev.sessao, &titulo, &projeto);
                }
                atual.pedido = titulo.clone();
                atual.legenda = if titulo.is_empty() { "Comecei um pedido".into() } else { titulo };
            } else {
                let leg = cortar(&ev.legenda, 120);
                atual.legenda = if !leg.is_empty() {
                    leg
                } else if ev.fase == "subagente" {
                    "Um funcionário terminou a parte dele".into()
                } else if !ev.ferramenta.is_empty() {
                    format!("Usando {}", cortar(&ev.ferramenta, 40))
                } else {
                    "Trabalhando".into()
                };
            }
            atual.estado = "trabalhando".into();
            atual.ferramenta = cortar(&ev.ferramenta, 40);
            if !projeto.is_empty() {
                atual.projeto = projeto;
            }
            atual.aviso = None;
            None
        }
        "precisa" => {
            let texto = cortar(&ev.texto, 300);
            let ocioso = {
                let t = texto.to_lowercase();
                t.contains("waiting for your input") || t.contains("aguardando")
            };
            // o "esperando você responder" depois de um pronto não apaga o aviso de pronto
            if ocioso && atual.estado == "pronto" {
                return None;
            }
            atual.estado = "precisa".into();
            atual.legenda = "Preciso de você".into();
            atual.aviso = Some(Aviso {
                tipo: "precisa".into(),
                topo: "SARA · PRECISO DE VOCÊ".into(),
                titulo: "Alex, preciso de você.".into(),
                texto: juntar_projeto(&projeto, if texto.is_empty() { "O Claude Code está esperando uma resposta sua." } else { &texto }),
                acao: "VER AGORA".into(),
                em: agora,
            });
            atual.som = true;
            Some(Notificar { titulo: "Alex, preciso de você".into(), corpo: atual.aviso.as_ref().unwrap().texto.clone() })
        }
        "pronto" => {
            // pronto repetido em menos de 10 s (hook + conversa do programa): não avisa duas vezes
            if atual.estado == "pronto" && atual.aviso.as_ref().map(|a| agora.saturating_sub(a.em) < 10_000).unwrap_or(false) {
                return None;
            }
            let lembrado = mem.pedido(&ev.sessao).cloned();
            let pedido = if !ev.titulo.trim().is_empty() {
                cortar(&ev.titulo, 140)
            } else if let Some((p, _)) = &lembrado {
                p.clone()
            } else {
                atual.pedido.clone()
            };
            let projeto = if !projeto.is_empty() { projeto } else { lembrado.map(|x| x.1).unwrap_or_default() };
            let resumo = cortar(&ev.resumo, 200);
            let texto = if !pedido.is_empty() {
                pedido.clone()
            } else if !resumo.is_empty() {
                resumo
            } else {
                "O Claude Code terminou o que você pediu.".into()
            };
            atual.estado = "pronto".into();
            atual.legenda = "Trabalho pronto".into();
            atual.pedido = pedido;
            atual.projeto = projeto.clone();
            atual.ferramenta.clear();
            atual.aviso = Some(Aviso {
                tipo: "pronto".into(),
                topo: "SARA · PRONTO".into(),
                titulo: "Alex, seu trabalho está pronto.".into(),
                texto: juntar_projeto(&projeto, &texto),
                acao: "VER AGORA".into(),
                em: agora,
            });
            atual.som = true;
            Some(Notificar { titulo: "Alex, seu trabalho está pronto".into(), corpo: atual.aviso.as_ref().unwrap().texto.clone() })
        }
        "erro" => {
            let texto = cortar(&ev.texto, 300);
            atual.estado = "erro".into();
            atual.legenda = "Preciso de você".into();
            atual.aviso = Some(Aviso {
                tipo: "erro".into(),
                topo: "SARA · PRECISO DE VOCÊ".into(),
                titulo: "Alex, travei numa parte.".into(),
                texto: juntar_projeto(&projeto, if texto.is_empty() { "Algo deu errado." } else { &texto }),
                acao: "VER O ERRO".into(),
                em: agora,
            });
            atual.som = true;
            Some(Notificar { titulo: "Alex, travei numa parte".into(), corpo: atual.aviso.as_ref().unwrap().texto.clone() })
        }
        _ => {
            atual.estado = "esperando".into();
            atual.legenda = "Por aqui".into();
            atual.aviso = None;
            None
        }
    }
}

pub fn dispensar(atual: &mut Estado) {
    atual.aviso = None;
    atual.som = false;
    if atual.estado != "trabalhando" {
        atual.estado = "esperando".into();
        atual.legenda = "Por aqui".into();
    }
    atual.em = agora_ms();
}

fn juntar_projeto(projeto: &str, texto: &str) -> String {
    if projeto.is_empty() {
        texto.to_string()
    } else {
        format!("{projeto}: {texto}")
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ev(tipo: &str) -> Evento {
        Evento { tipo: tipo.into(), sessao: "s1".into(), projeto: "curso-mirian".into(), ..Default::default() }
    }

    #[test]
    fn pedido_vira_resumo_do_pronto() {
        let mut e = Estado::default();
        let mut m = Memoria::default();
        let mut p = ev("trabalhando");
        p.fase = "pedido".into();
        p.titulo = "Faz a landing\n do curso".into();
        assert!(aplicar(&mut e, &mut m, &p).is_none());
        assert_eq!(e.estado, "trabalhando");
        assert_eq!(e.legenda, "Faz a landing do curso");
        let mut f = ev("trabalhando");
        f.fase = "ferramenta".into();
        f.legenda = "Lendo LEIA-ME.md".into();
        aplicar(&mut e, &mut m, &f);
        assert_eq!(e.legenda, "Lendo LEIA-ME.md");
        let n = aplicar(&mut e, &mut m, &ev("pronto")).unwrap();
        assert_eq!(n.titulo, "Alex, seu trabalho está pronto");
        assert_eq!(n.corpo, "curso-mirian: Faz a landing do curso");
        assert!(e.som);
        assert_eq!(e.aviso.as_ref().unwrap().titulo, "Alex, seu trabalho está pronto.");
        // repetido logo em seguida: sem segunda notificação
        assert!(aplicar(&mut e, &mut m, &ev("pronto")).is_none());
    }

    #[test]
    fn ocioso_nao_apaga_pronto() {
        let mut e = Estado::default();
        let mut m = Memoria::default();
        aplicar(&mut e, &mut m, &ev("pronto"));
        let mut n = ev("precisa");
        n.texto = "Claude is waiting for your input".into();
        assert!(aplicar(&mut e, &mut m, &n).is_none());
        assert_eq!(e.estado, "pronto");
        n.texto = "Claude needs your permission to use Bash".into();
        assert!(aplicar(&mut e, &mut m, &n).is_some());
        assert_eq!(e.estado, "precisa");
    }

    #[test]
    fn dispensar_volta_a_esperar() {
        let mut e = Estado::default();
        let mut m = Memoria::default();
        aplicar(&mut e, &mut m, &ev("pronto"));
        dispensar(&mut e);
        assert!(e.aviso.is_none());
        assert_eq!(e.estado, "esperando");
    }

    #[test]
    fn corta_texto_longo() {
        let t = "a".repeat(500);
        assert_eq!(cortar(&t, 10).chars().count(), 10);
    }
}
