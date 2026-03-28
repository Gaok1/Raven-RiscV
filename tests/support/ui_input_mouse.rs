
use super::*;

#[test]
fn run_status_hit_accounts_for_core_prefix() {
    let app = App::new(None);
    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));

    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| run_status_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::Core));
    assert!(hits.contains(&RunButton::View));
    assert!(hits.contains(&RunButton::Format));
    assert!(hits.contains(&RunButton::Reset));
}

#[test]
fn run_status_hit_hides_region_and_bytes_in_dyn_view() {
    let mut app = App::new(None);
    app.run.show_dyn = true;

    let status = run_status_area(&app, Rect::new(0, 0, 160, 40));
    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| run_status_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::View));
    assert!(!hits.contains(&RunButton::Region));
    assert!(!hits.contains(&RunButton::Bytes));
}

#[test]
fn cache_exec_hit_exposes_reset_speed_and_state() {
    let app = App::new(None);
    let status = cache_run_status_area(Rect::new(0, 0, 160, 40));

    let hits: Vec<RunButton> = (status.x..status.x + status.width)
        .filter_map(|col| cache_exec_hit(&app, status, col))
        .collect();

    assert!(hits.contains(&RunButton::Reset));
    assert!(hits.contains(&RunButton::Speed));
    assert!(hits.contains(&RunButton::State));
}
