//! Contador de frequÃªncia de alvos de desvios tomados (*hot profile*).
//!
//! # O que Ã© e para que serve
//!
//! `HotProfile` mantÃ©m um mapa `PC â†’ contagem` que registra quantas vezes
//! cada endereÃ§o foi **alvo de um branch ou jump tomado**. O backend `hot`
//! da Fase C consulta esses contadores para decidir quais basic blocks compilar:
//! quando um PC atinge o threshold de 500 entradas, o bloco a partir daquele
//! PC Ã© compilado e inserido no [`CompiledBlockCache`].
//!
//! # Por que "alvo de desvio tomado" e nÃ£o "toda instruÃ§Ã£o executada"?
//!
//! PCs que sÃ£o alvos frequentes de desvios tomados sÃ£o, por definiÃ§Ã£o,
//! cabeÃ§as de loops quentes â€” exatamente o que o JIT quer compilar.
//! Rastrear toda instruÃ§Ã£o (como a TUI faz com `exec_counts`) geraria muito
//! mais ruÃ­do e nÃ£o seria mais Ãºtil para a decisÃ£o de compilaÃ§Ã£o.
//!
//! # DiferenÃ§a em relaÃ§Ã£o ao `exec_counts` da TUI
//!
//! | Campo                   | Onde vive       | O que conta                      |
//! |-------------------------|-----------------|----------------------------------|
//! | `HotProfile`            | JIT backend     | Alvos de desvios tomados         |
//! | `app.run.exec_counts`   | TUI (`hart.rs`) | Toda instruÃ§Ã£o executada, por PC |
//!
//! SÃ£o estruturas independentes com propÃ³sitos distintos.
//!
//! [`CompiledBlockCache`]: super::cache::CompiledBlockCache

use std::collections::HashMap;

/// Rastreia a frequÃªncia de PCs que sÃ£o alvos de branches/jumps tomados.
pub struct HotProfile {
    counts: HashMap<u32, u32>,
}

impl HotProfile {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// Incrementa o contador do PC `pc` (saturando em `u32::MAX`).
    #[inline]
    pub fn record_target(&mut self, pc: u32) {
        let slot = self.counts.entry(pc).or_insert(0);

        *slot = slot.saturating_add(1);
    }

    /// Retorna o contador atual do PC `pc`, ou 0 se nunca registrado.
    pub fn get(&self, pc: u32) -> u32 {
        self.counts.get(&pc).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &u32)> {
        self.counts.iter()
    }

    pub fn len(&self) -> usize {
        self.counts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn clear(&mut self) {
        self.counts.clear();
    }
}

impl Default for HotProfile {
    fn default() -> Self {
        Self::new()
    }
}

