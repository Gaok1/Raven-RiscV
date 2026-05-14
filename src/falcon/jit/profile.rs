//! Contador de frequência de alvos de desvios tomados (*hot profile*).
//!
//! # O que é e para que serve
//!
//! `HotProfile` mantém um mapa `PC → contagem` que registra quantas vezes
//! cada endereço foi **alvo de um branch ou jump tomado**. O backend `hot`
//! da Fase C consulta esses contadores para decidir quais basic blocks compilar:
//! quando um PC atinge o threshold de 500 entradas, o bloco a partir daquele
//! PC é compilado e inserido no [`CompiledBlockCache`].
//!
//! # Por que "alvo de desvio tomado" e não "toda instrução executada"?
//!
//! PCs que são alvos frequentes de desvios tomados são, por definição,
//! cabeças de loops quentes — exatamente o que o JIT quer compilar.
//! Rastrear toda instrução (como a TUI faz com `exec_counts`) geraria muito
//! mais ruído e não seria mais útil para a decisão de compilação.
//!
//! # Diferença em relação ao `exec_counts` da TUI
//!
//! | Campo                   | Onde vive       | O que conta                      |
//! |-------------------------|-----------------|----------------------------------|
//! | `HotProfile`            | JIT backend     | Alvos de desvios tomados         |
//! | `app.run.exec_counts`   | TUI (`hart.rs`) | Toda instrução executada, por PC |
//!
//! São estruturas independentes com propósitos distintos.
//!
//! [`CompiledBlockCache`]: super::cache::CompiledBlockCache

use std::collections::HashMap;

/// Rastreia a frequência de PCs que são alvos de branches/jumps tomados.
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
