//! Gestos opcionais (documento EMPETO, 7.1) — `GestureEvent`.
//!
//! A interface só informa *qual* gesto aconteceu e em *qual* contexto; a decisão sobre a ação
//! fica aqui, num único lugar. Todo gesto tem um equivalente por botão: desligar o módulo de
//! gestos nunca remove uma funcionalidade.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureEvent {
    SwipeLeft,
    SwipeRight,
}

impl GestureEvent {
    pub fn parse(s: &str) -> Option<GestureEvent> {
        match s {
            "swipe-left" => Some(GestureEvent::SwipeLeft),
            "swipe-right" => Some(GestureEvent::SwipeRight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureContext {
    /// Modo foco: uma série por vez.
    Focus,
    /// Calendário mensal/semanal.
    Calendar,
    /// Tela de detalhe da evolução (troca de período).
    Evolution,
}

impl GestureContext {
    pub fn parse(s: &str) -> Option<GestureContext> {
        match s {
            "focus" => Some(GestureContext::Focus),
            "calendar" => Some(GestureContext::Calendar),
            "evolution" => Some(GestureContext::Evolution),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureAction {
    NextSet,
    PrevSet,
    NextPeriod,
    PrevPeriod,
}

/// Mapeia gesto + contexto para a ação correspondente. `enabled` = módulo de gestos ligado.
pub fn resolve(enabled: bool, ctx: GestureContext, ev: GestureEvent) -> Option<GestureAction> {
    if !enabled {
        return None;
    }
    use GestureAction::*;
    use GestureEvent::*;
    Some(match (ctx, ev) {
        (GestureContext::Focus, SwipeLeft) => NextSet,
        (GestureContext::Focus, SwipeRight) => PrevSet,
        (GestureContext::Calendar | GestureContext::Evolution, SwipeLeft) => NextPeriod,
        (GestureContext::Calendar | GestureContext::Evolution, SwipeRight) => PrevPeriod,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_and_disabled() {
        assert_eq!(resolve(true, GestureContext::Focus, GestureEvent::SwipeLeft), Some(GestureAction::NextSet));
        assert_eq!(resolve(true, GestureContext::Calendar, GestureEvent::SwipeRight), Some(GestureAction::PrevPeriod));
        assert_eq!(resolve(false, GestureContext::Focus, GestureEvent::SwipeLeft), None);
        assert_eq!(GestureEvent::parse("x"), None);
        assert_eq!(GestureContext::parse("focus"), Some(GestureContext::Focus));
    }
}
