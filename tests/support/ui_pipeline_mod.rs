use super::{
    BranchPredict, BranchResolve, GanttCell, GanttRow, InstrClass, PipelineBypassConfig,
    PipelineConfig, PipelineMode, PipelineSimState, PipelineSpeed, Stage, gantt_max_scroll,
    gantt_window_bounds, maybe_follow_gantt_tail, parse_pipeline_config,
    serialize_pipeline_config,
};
use std::collections::VecDeque;

#[test]
fn pipeline_config_roundtrip() {
    let cfg = PipelineConfig {
        enabled: false,
        bypass: PipelineBypassConfig::new(false, true, false, true),
        branch_resolve: BranchResolve::Mem,
        mode: PipelineMode::FunctionalUnits,
        predict: BranchPredict::TwoBit,
        speed: PipelineSpeed::Fast,
    };

    let text = serialize_pipeline_config(&cfg);
    let parsed = parse_pipeline_config(&text).expect("parse pipeline config");
    assert_eq!(parsed, cfg);
}

#[test]
fn pipeline_config_parses_legacy_forwarding_and_new_predictors() {
    let parsed = parse_pipeline_config(
        "enabled=true\nforwarding=false\nmode=functional_units\nbranch_resolve=id\npredict=btfnt\nspeed=fast\n",
    )
    .expect("parse legacy pipeline config");

    assert_eq!(parsed.bypass, PipelineBypassConfig::disabled());
    assert_eq!(parsed.predict, BranchPredict::Btfnt);

    let parsed = parse_pipeline_config(
        "enabled=true\nbypass.ex_to_ex=true\nbypass.mem_to_ex=false\nbypass.wb_to_id=true\nbypass.store_to_load=true\npredict=twobit\n",
    )
    .expect("parse granular bypass config");
    assert_eq!(
        parsed.bypass,
        PipelineBypassConfig::new(true, false, true, true)
    );
    assert_eq!(parsed.predict, BranchPredict::TwoBit);
}

#[test]
fn pipeline_config_ignores_removed_collapse_cache_stalls_field() {
    let parsed = parse_pipeline_config(
        "enabled=true\npredict=twobit\ncollapse_cache_stalls=on\nspeed=fast\n",
    )
    .expect("parse config with removed field");

    assert!(parsed.enabled);
    assert_eq!(parsed.predict, BranchPredict::TwoBit);
    assert_eq!(parsed.speed, PipelineSpeed::Fast);
}

#[test]
fn gantt_window_includes_all_cycles_when_history_is_wide_enough() {
    let rows = vec![
        GanttRow {
            gantt_id: 1,
            pc: 0,
            disasm: "addi".into(),
            class: InstrClass::Alu,
            cells: VecDeque::from(vec![GanttCell::InStage(Stage::IF); 5]),
            first_cycle: 10,
            done: false,
            last_stage: None,
        },
        GanttRow {
            gantt_id: 2,
            pc: 4,
            disasm: "jal".into(),
            class: InstrClass::Jump,
            cells: VecDeque::from(vec![GanttCell::InStage(Stage::ID); 4]),
            first_cycle: 14,
            done: true,
            last_stage: None,
        },
    ];
    let refs: Vec<_> = rows.iter().collect();
    let (start, end) = gantt_window_bounds(&refs, 20);
    assert_eq!(start, 10);
    assert_eq!(end, 18);
}

#[test]
fn gantt_window_caps_to_requested_history_width_from_the_newest_cycles() {
    let rows = vec![GanttRow {
        gantt_id: 1,
        pc: 0,
        disasm: "addi".into(),
        class: InstrClass::Alu,
        cells: VecDeque::from(vec![GanttCell::InStage(Stage::IF); 50]),
        first_cycle: 30,
        done: false,
        last_stage: None,
    }];
    let refs: Vec<_> = rows.iter().collect();
    let (start, end) = gantt_window_bounds(&refs, 12);
    assert_eq!(start, 68);
    assert_eq!(end, 80);
}

#[test]
fn gantt_max_scroll_matches_visible_history_capacity() {
    let mut state = PipelineSimState::new();
    state.gantt = (0..10)
        .map(|i| GanttRow {
            gantt_id: i + 1,
            pc: (i * 4) as u32,
            disasm: format!("addi x{i}, x{i}, 1"),
            class: InstrClass::Alu,
            cells: VecDeque::from(vec![GanttCell::InStage(Stage::IF); 4]),
            first_cycle: i as u64,
            done: false,
            last_stage: None,
        })
        .collect();

    assert_eq!(gantt_max_scroll(&state, 7), 7);
}

#[test]
fn gantt_tail_follow_advances_when_newest_row_is_visible() {
    assert_eq!(maybe_follow_gantt_tail(2, 3, 5), 3);
}

#[test]
fn gantt_tail_follow_stays_put_when_newest_row_is_hidden() {
    assert_eq!(maybe_follow_gantt_tail(1, 3, 5), 1);
}
