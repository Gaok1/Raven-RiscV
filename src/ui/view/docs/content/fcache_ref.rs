use super::common::{blank, h1, h2, kv, mono, note, raw, tsep};
use crate::ui::app::DocsLang;
use ratatui::prelude::*;

pub(crate) fn fcache_ref_lines(lang: DocsLang) -> Vec<Line<'static>> {
    match lang {
        DocsLang::En => fcache_ref_lines_en(),
        DocsLang::PtBr => fcache_ref_lines_ptbr(),
    }
}

fn fcache_ref_lines_en() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — .fcache Config Reference"),
        blank(),
        note("A .fcache file stores cache hierarchy and CPI settings for sharing and reloading."),
        note("Use Ctrl+e to export and Ctrl+l to import on the Cache tab."),
        blank(),
        h2("Format Rules"),
        blank(),
        raw("  • Lines starting with # are comments and are ignored."),
        raw("  • Each setting is a key=value pair on its own line."),
        raw("  • Line order does not matter."),
        raw("  • Unknown keys are silently ignored (forward-compatible)."),
        raw("  • CPI keys are optional — missing keys use the default values shown below."),
        blank(),
        h2("Level Prefixes"),
        blank(),
        kv("icache", "L1 Instruction Cache"),
        kv("dcache", "L1 Data Cache"),
        kv("l2", "Level 2 unified cache"),
        kv("l3", "Level 3 unified cache"),
        kv("l4", "Level 4 unified cache"),
        kv(
            "levels=N",
            "Number of extra levels beyond L1 (0 = L1 only, 1 = L1+L2, …)",
        ),
        blank(),
        h2("Cache Level Keys  (prefix.key=value)"),
        blank(),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", "Key suffix"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Type"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<28}", "Valid values / range"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Default"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Notes",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        tsep(),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".size"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "64 – 1 048 576 bytes, pow2"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("Total cache size", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".line_size"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "4 – 512 bytes, pow2"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Cache line / block size",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".associativity"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "1 – 16"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "1=direct-mapped, N=N-way",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".replacement"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "Lru Mru Fifo Random Lfu Clock"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("Eviction policy", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".write_policy"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "WriteBack WriteThrough"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".write_alloc"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "WriteAllocate NoWriteAllocate"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".inclusion"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "NonInclusive Inclusive Exclusive"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<10}", "NonInclusive"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("optional", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".hit_latency"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "1 – 999 cycles"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".miss_penalty"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "0 – 9999 cycles"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("Stall cycles on miss", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".assoc_penalty"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "0 – 99"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "1"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Extra cyc/way tag search",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".transfer_width"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "1 – 512 bytes"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "8"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Bus width for line xfer",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        blank(),
        h2("CPI Config Keys"),
        blank(),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "Key"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Type"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Default"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Description",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        tsep(),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.alu"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "1"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "add/sub/and/or/xor/shift/lui/auipc",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.mul"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "3"), Style::default().fg(Color::DarkGray)),
            Span::styled("mul/mulh/mulhsu/mulhu", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.div"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<10}", "20"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("div/divu/rem/remu", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.load"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "0"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "extra load overhead (beyond cache latency)",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.store"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "0"), Style::default().fg(Color::DarkGray)),
            Span::styled("extra store overhead", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.branch_taken"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "3"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "branch when taken (pipeline flush)",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.branch_not_taken"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "1"), Style::default().fg(Color::DarkGray)),
            Span::styled("branch when not taken", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.jump"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "2"), Style::default().fg(Color::DarkGray)),
            Span::styled("jal / jalr", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.system"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<10}", "10"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("ecall / ebreak / halt", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.fp"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "integer"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "5"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "RV32F float instructions",
                Style::default().fg(Color::White),
            ),
        ]),
        blank(),
        h2("Annotated Example File"),
        blank(),
        mono("  # Raven Cache Config v2"),
        mono("  levels=1"),
        blank(),
        mono("  icache.size=4096"),
        mono("  icache.line_size=32"),
        mono("  icache.associativity=2"),
        mono("  icache.replacement=Lru"),
        mono("  icache.write_policy=WriteBack"),
        mono("  icache.write_alloc=WriteAllocate"),
        mono("  icache.hit_latency=1"),
        mono("  icache.miss_penalty=50"),
        mono("  icache.assoc_penalty=1"),
        mono("  icache.transfer_width=8"),
        blank(),
        mono("  dcache.size=4096"),
        mono("  dcache.line_size=32"),
        mono("  dcache.associativity=4"),
        mono("  dcache.replacement=Lru"),
        mono("  dcache.write_policy=WriteBack"),
        mono("  dcache.write_alloc=WriteAllocate"),
        mono("  dcache.hit_latency=2"),
        mono("  dcache.miss_penalty=50"),
        mono("  dcache.assoc_penalty=1"),
        mono("  dcache.transfer_width=8"),
        blank(),
        mono("  l2.size=131072"),
        mono("  l2.line_size=64"),
        mono("  l2.associativity=8"),
        mono("  l2.replacement=Lru"),
        mono("  l2.write_policy=WriteBack"),
        mono("  l2.write_alloc=WriteAllocate"),
        mono("  l2.inclusion=NonInclusive"),
        mono("  l2.hit_latency=10"),
        mono("  l2.miss_penalty=200"),
        mono("  l2.assoc_penalty=2"),
        mono("  l2.transfer_width=16"),
        blank(),
        mono("  # --- CPI Config ---"),
        mono("  cpi.alu=1"),
        mono("  cpi.mul=3"),
        mono("  cpi.div=20"),
        mono("  cpi.load=0"),
        mono("  cpi.store=0"),
        mono("  cpi.branch_taken=3"),
        mono("  cpi.branch_not_taken=1"),
        mono("  cpi.jump=2"),
        mono("  cpi.system=10"),
        mono("  cpi.fp=5"),
        blank(),
    ]
}

fn fcache_ref_lines_ptbr() -> Vec<Line<'static>> {
    vec![
        h1("RAVEN — Referência de Configuração .fcache"),
        blank(),
        note(
            "Um arquivo .fcache armazena configurações de hierarquia de cache e CPI para compartilhar e recarregar.",
        ),
        note("Use Ctrl+e para exportar e Ctrl+l para importar na aba Cache."),
        blank(),
        h2("Regras de Formato"),
        blank(),
        raw("  • Linhas começando com # são comentários e são ignoradas."),
        raw("  • Cada configuração é um par chave=valor em uma linha própria."),
        raw("  • A ordem das linhas não importa."),
        raw(
            "  • Chaves desconhecidas são silenciosamente ignoradas (compatível com versões futuras).",
        ),
        raw(
            "  • Chaves CPI são opcionais — chaves ausentes usam os valores padrão mostrados abaixo.",
        ),
        blank(),
        h2("Prefixos de Nível"),
        blank(),
        kv("icache", "Cache de Instruções L1"),
        kv("dcache", "Cache de Dados L1"),
        kv("l2", "Cache unificado nível 2"),
        kv("l3", "Cache unificado nível 3"),
        kv("l4", "Cache unificado nível 4"),
        kv(
            "levels=N",
            "Número de níveis extras além do L1 (0 = só L1, 1 = L1+L2, …)",
        ),
        blank(),
        h2("Chaves de Nível de Cache  (prefixo.chave=valor)"),
        blank(),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", "Sufixo da chave"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Tipo"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<28}", "Valores válidos / intervalo"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Padrão"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Notas",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        tsep(),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".size"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "64 – 1 048 576 bytes, pot2"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Tamanho total do cache",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".line_size"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "4 – 512 bytes, pot2"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Tamanho da linha de cache",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".associativity"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "1 – 16"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "1=mapeamento direto, N=N-way",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".replacement"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "Lru Mru Fifo Random Lfu Clock"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Política de substituição",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".write_policy"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "WriteBack WriteThrough"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".write_alloc"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "WriteAllocate NoWriteAllocate"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".inclusion"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<10}", "enum"), Style::default().fg(Color::White)),
            Span::styled(
                format!("{:<28}", "NonInclusive Inclusive Exclusive"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<10}", "NonInclusive"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("opcional", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".hit_latency"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "1 – 999 ciclos"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled("", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".miss_penalty"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "0 – 9999 ciclos"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "—"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Ciclos de espera em miss",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".assoc_penalty"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "0 – 99"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "1"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Ciclos extras/via na busca",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", ".transfer_width"),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<28}", "1 – 512 bytes"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "8"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Largura do barramento",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        blank(),
        h2("Chaves de Configuração CPI"),
        blank(),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "Chave"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Tipo"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<10}", "Padrão"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Descrição",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        tsep(),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.alu"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "1"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "add/sub/and/or/xor/shift/lui/auipc",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.mul"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "3"), Style::default().fg(Color::DarkGray)),
            Span::styled("mul/mulh/mulhsu/mulhu", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.div"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<10}", "20"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("div/divu/rem/remu", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.load"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "0"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "overhead extra de load (além da latência de cache)",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.store"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "0"), Style::default().fg(Color::DarkGray)),
            Span::styled("overhead extra de store", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.branch_taken"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "3"), Style::default().fg(Color::DarkGray)),
            Span::styled(
                "branch tomado (flush do pipeline)",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.branch_not_taken"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "1"), Style::default().fg(Color::DarkGray)),
            Span::styled("branch não tomado", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.jump"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "2"), Style::default().fg(Color::DarkGray)),
            Span::styled("jal / jalr", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.system"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<10}", "10"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("ecall / ebreak / halt", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {:<22}", "cpi.fp"),
                Style::default().fg(Color::LightCyan),
            ),
            Span::styled(
                format!("{:<10}", "inteiro"),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<10}", "5"), Style::default().fg(Color::DarkGray)),
            Span::styled("Instruções float RV32F", Style::default().fg(Color::White)),
        ]),
        blank(),
        h2("Exemplo Anotado de Arquivo"),
        blank(),
        mono("  # Raven Cache Config v2"),
        mono("  levels=1"),
        blank(),
        mono("  icache.size=4096"),
        mono("  icache.line_size=32"),
        mono("  icache.associativity=2"),
        mono("  icache.replacement=Lru"),
        mono("  icache.write_policy=WriteBack"),
        mono("  icache.write_alloc=WriteAllocate"),
        mono("  icache.hit_latency=1"),
        mono("  icache.miss_penalty=50"),
        mono("  icache.assoc_penalty=1"),
        mono("  icache.transfer_width=8"),
        blank(),
        mono("  dcache.size=4096"),
        mono("  dcache.line_size=32"),
        mono("  dcache.associativity=4"),
        mono("  dcache.replacement=Lru"),
        mono("  dcache.write_policy=WriteBack"),
        mono("  dcache.write_alloc=WriteAllocate"),
        mono("  dcache.hit_latency=2"),
        mono("  dcache.miss_penalty=50"),
        mono("  dcache.assoc_penalty=1"),
        mono("  dcache.transfer_width=8"),
        blank(),
        mono("  l2.size=131072"),
        mono("  l2.line_size=64"),
        mono("  l2.associativity=8"),
        mono("  l2.replacement=Lru"),
        mono("  l2.write_policy=WriteBack"),
        mono("  l2.write_alloc=WriteAllocate"),
        mono("  l2.inclusion=NonInclusive"),
        mono("  l2.hit_latency=10"),
        mono("  l2.miss_penalty=200"),
        mono("  l2.assoc_penalty=2"),
        mono("  l2.transfer_width=16"),
        blank(),
        mono("  # --- CPI Config ---"),
        mono("  cpi.alu=1"),
        mono("  cpi.mul=3"),
        mono("  cpi.div=20"),
        mono("  cpi.load=0"),
        mono("  cpi.store=0"),
        mono("  cpi.branch_taken=3"),
        mono("  cpi.branch_not_taken=1"),
        mono("  cpi.jump=2"),
        mono("  cpi.system=10"),
        mono("  cpi.fp=5"),
        blank(),
    ]
}
