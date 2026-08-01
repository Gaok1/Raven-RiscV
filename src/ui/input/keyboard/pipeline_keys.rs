use crate::ui::app::App;
use crate::ui::pipeline::PipelineBypassConfig;
use crossterm::event::{KeyCode, KeyEvent};
use std::time::Instant;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Tab => {
            use crate::ui::pipeline::PipelineSubtab;
            app.run.pipeline_view_mut().clear_hover_state();
            app.run.pipeline_view_mut().subtab = match app.run.pipeline_view_mut().subtab {
                PipelineSubtab::Main => PipelineSubtab::Config,
                PipelineSubtab::Config => PipelineSubtab::Main,
            };
            true
        }
        KeyCode::Char('e') => {
            app.run.pipeline_view_mut().clear_hover_state();
            app.set_pipeline_enabled(!app.pipeline_status().is_some_and(|status| status.enabled));
            true
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.run.pipeline_view_mut().clear_hover_state();
            app.restart_simulation();
            true
        }
        KeyCode::Char('f') => {
            app.run.pipeline_view_mut().clear_hover_state();
            app.run.pipeline_view_mut().speed = app.run.pipeline_view_mut().speed.next();
            app.run.pipeline_view_mut().last_tick = Instant::now();
            true
        }
        KeyCode::Char('b') => {
            use crate::ui::pipeline::BranchResolve;
            app.run.pipeline_view_mut().clear_hover_state();
            if let Some(pipeline) = app.pipeline_config_mut() {
                pipeline.branch_resolve = match pipeline.branch_resolve {
                    BranchResolve::Id => BranchResolve::Ex,
                    BranchResolve::Ex => BranchResolve::Mem,
                    BranchResolve::Mem => BranchResolve::Id,
                };
                app.reconfigure_pipeline_model();
            }
            true
        }
        KeyCode::Char('s') => {
            app.run.pipeline_view_mut().clear_hover_state();
            if app.pipeline_status().is_some_and(|status| (status.enabled || status.sequential) && !status.faulted) {
                app.single_step();
            }
            true
        }
        KeyCode::Char('p') | KeyCode::Char(' ')
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            if app.pipeline_status().is_some_and(|status| (status.enabled || status.sequential) && !status.faulted) {
                if app.pipeline_status().is_some_and(|status| status.halted) {
                    app.restart_simulation();
                    if app.can_start_run() {
                        app.run.is_running = true;
                    }
                } else {
                    app.resume_selected_hart();
                    if app.run.is_running {
                        app.run.is_running = false;
                    } else if app.can_start_run() {
                        app.run.is_running = true;
                    }
                }
            }
            true
        }
        KeyCode::Enter
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Config
            ) =>
        {
            use crate::ui::pipeline::{BranchPredict, BranchResolve, PipelineMode};
            app.run.pipeline_view_mut().clear_hover_state();
            let cursor = app.run.pipeline_view().config_cursor;
            let Some(pipeline) = app.pipeline_config_mut() else {
                return true;
            };
            match cursor {
                0 => pipeline.bypass.ex_to_ex = !pipeline.bypass.ex_to_ex,
                1 => pipeline.bypass.mem_to_ex = !pipeline.bypass.mem_to_ex,
                2 => pipeline.bypass.wb_to_id = !pipeline.bypass.wb_to_id,
                3 => pipeline.bypass.store_to_load = !pipeline.bypass.store_to_load,
                4 => {
                    pipeline.mode = match pipeline.mode {
                        PipelineMode::SingleCycle => PipelineMode::FunctionalUnits,
                        PipelineMode::FunctionalUnits => PipelineMode::SingleCycle,
                    };
                }
                5 => {
                    pipeline.branch_resolve = match pipeline.branch_resolve {
                        BranchResolve::Id => BranchResolve::Ex,
                        BranchResolve::Ex => BranchResolve::Mem,
                        BranchResolve::Mem => BranchResolve::Id,
                    };
                }
                6 => {
                    let next = match pipeline.predict {
                        BranchPredict::NotTaken => BranchPredict::Taken,
                        BranchPredict::Taken => BranchPredict::Btfnt,
                        BranchPredict::Btfnt => BranchPredict::TwoBit,
                        BranchPredict::TwoBit => BranchPredict::NotTaken,
                    };
                    pipeline.set_predict(next);
                }
                7 => {
                    let idx = crate::ui::pipeline::FuKind::Alu.index();
                    pipeline.fu_capacity[idx] = if pipeline.fu_capacity[idx] >= 8 {
                        1
                    } else {
                        pipeline.fu_capacity[idx] + 1
                    };
                }
                8 => {
                    let idx = crate::ui::pipeline::FuKind::Mul.index();
                    pipeline.fu_capacity[idx] = if pipeline.fu_capacity[idx] >= 8 {
                        1
                    } else {
                        pipeline.fu_capacity[idx] + 1
                    };
                }
                9 => {
                    let idx = crate::ui::pipeline::FuKind::Div.index();
                    pipeline.fu_capacity[idx] = if pipeline.fu_capacity[idx] >= 8 {
                        1
                    } else {
                        pipeline.fu_capacity[idx] + 1
                    };
                }
                10 => {
                    let idx = crate::ui::pipeline::FuKind::Fpu.index();
                    pipeline.fu_capacity[idx] = if pipeline.fu_capacity[idx] >= 8 {
                        1
                    } else {
                        pipeline.fu_capacity[idx] + 1
                    };
                }
                11 => {
                    let idx = crate::ui::pipeline::FuKind::Lsu.index();
                    pipeline.fu_capacity[idx] = if pipeline.fu_capacity[idx] >= 8 {
                        1
                    } else {
                        pipeline.fu_capacity[idx] + 1
                    };
                }
                12 => {
                    let idx = crate::ui::pipeline::FuKind::Sys.index();
                    pipeline.fu_capacity[idx] = if pipeline.fu_capacity[idx] >= 8 {
                        1
                    } else {
                        pipeline.fu_capacity[idx] + 1
                    };
                }
                _ => {}
            }
            app.reconfigure_pipeline_model();
            true
        }
        KeyCode::Up
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Config
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            app.run.pipeline_view_mut().config_cursor = app.run.pipeline_view_mut().config_cursor.saturating_sub(1);
            true
        }
        KeyCode::Down
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Config
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            app.run.pipeline_view_mut().config_cursor =
                (app.run.pipeline_view_mut().config_cursor + 1).min(PipelineBypassConfig::CONFIG_ROWS - 1);
            true
        }
        // Gantt scroll is bottom-anchored: 0 = follow the newest row, so Up
        // moves *into* scrollback (larger offset) and Down back toward follow.
        KeyCode::Up
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            let max = app.run.pipeline_view().gantt_max_scroll_cache.get();
            app.run.pipeline_view_mut().gantt_scroll = (app.run.pipeline_view_mut().gantt_scroll + 1).min(max);
            true
        }
        KeyCode::Down
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            app.run.pipeline_view_mut().gantt_scroll = app.run.pipeline_view_mut().gantt_scroll.saturating_sub(1);
            true
        }
        KeyCode::PageUp
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            let page = app.run.pipeline_view().gantt_visible_rows_cache.get().max(1);
            let max = app.run.pipeline_view().gantt_max_scroll_cache.get();
            app.run.pipeline_view_mut().gantt_scroll = app.run.pipeline_view_mut().gantt_scroll.saturating_add(page).min(max);
            true
        }
        KeyCode::PageDown
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            let page = app.run.pipeline_view().gantt_visible_rows_cache.get().max(1);
            app.run.pipeline_view_mut().gantt_scroll = app.run.pipeline_view_mut().gantt_scroll.saturating_sub(page);
            true
        }
        KeyCode::End | KeyCode::Char('G')
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            app.run.pipeline_view_mut().gantt_scroll = 0;
            true
        }
        KeyCode::Home | KeyCode::Char('g')
            if matches!(
                app.run.pipeline_view().subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            app.run.pipeline_view_mut().clear_hover_state();
            let max = app.run.pipeline_view().gantt_max_scroll_cache.get();
            app.run.pipeline_view_mut().gantt_scroll = max;
            true
        }
        _ => false,
    }
}
