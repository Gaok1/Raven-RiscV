use crate::ui::app::App;
use crate::ui::pipeline::PipelineBypassConfig;
use crossterm::event::{KeyCode, KeyEvent};
use std::time::Instant;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Tab => {
            use crate::ui::pipeline::PipelineSubtab;
            app.pipeline.subtab = match app.pipeline.subtab {
                PipelineSubtab::Main => PipelineSubtab::Config,
                PipelineSubtab::Config => PipelineSubtab::Main,
            };
            true
        }
        KeyCode::Char('e') => {
            app.set_pipeline_enabled(!app.pipeline.enabled);
            true
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.restart_simulation();
            true
        }
        KeyCode::Char('f') => {
            app.pipeline.speed = app.pipeline.speed.next();
            app.pipeline.last_tick = Instant::now();
            true
        }
        KeyCode::Char('b') => {
            use crate::ui::pipeline::BranchResolve;
            app.pipeline.branch_resolve = match app.pipeline.branch_resolve {
                BranchResolve::Id => BranchResolve::Ex,
                BranchResolve::Ex => BranchResolve::Mem,
                BranchResolve::Mem => BranchResolve::Id,
            };
            app.reconfigure_pipeline_model();
            true
        }
        KeyCode::Char('s') => {
            if app.pipeline.enabled && !app.pipeline.faulted {
                app.single_step();
            }
            true
        }
        KeyCode::Char('p') | KeyCode::Char(' ')
            if matches!(
                app.pipeline.subtab,
                crate::ui::pipeline::PipelineSubtab::Main
            ) =>
        {
            if app.pipeline.enabled && !app.pipeline.faulted {
                if app.pipeline.halted {
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
                app.pipeline.subtab,
                crate::ui::pipeline::PipelineSubtab::Config
            ) =>
        {
            use crate::ui::pipeline::{BranchPredict, BranchResolve, PipelineMode};
            match app.pipeline.config_cursor {
                0 => app.pipeline.bypass.ex_to_ex = !app.pipeline.bypass.ex_to_ex,
                1 => app.pipeline.bypass.mem_to_ex = !app.pipeline.bypass.mem_to_ex,
                2 => app.pipeline.bypass.wb_to_id = !app.pipeline.bypass.wb_to_id,
                3 => app.pipeline.bypass.store_to_load = !app.pipeline.bypass.store_to_load,
                4 => {
                    app.pipeline.mode = match app.pipeline.mode {
                        PipelineMode::SingleCycle => PipelineMode::FunctionalUnits,
                        PipelineMode::FunctionalUnits => PipelineMode::SingleCycle,
                    };
                }
                5 => {
                    app.pipeline.branch_resolve = match app.pipeline.branch_resolve {
                        BranchResolve::Id => BranchResolve::Ex,
                        BranchResolve::Ex => BranchResolve::Mem,
                        BranchResolve::Mem => BranchResolve::Id,
                    };
                }
                6 => {
                    let next = match app.pipeline.predict {
                        BranchPredict::NotTaken => BranchPredict::Taken,
                        BranchPredict::Taken => BranchPredict::Btfnt,
                        BranchPredict::Btfnt => BranchPredict::TwoBit,
                        BranchPredict::TwoBit => BranchPredict::NotTaken,
                    };
                    app.pipeline.set_predict(next);
                }
                _ => {}
            }
            app.reconfigure_pipeline_model();
            true
        }
        KeyCode::Up => {
            app.pipeline.config_cursor = app.pipeline.config_cursor.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            app.pipeline.config_cursor =
                (app.pipeline.config_cursor + 1).min(PipelineBypassConfig::CONFIG_ROWS - 1);
            true
        }
        _ => false,
    }
}
