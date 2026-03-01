use super::App;

pub(super) fn imem_address_in_range(app: &App, addr: u32) -> bool {
    if let Some(text) = &app.editor.last_ok_text {
        let start = app.run.base_pc;
        let end = start.saturating_add((text.len() as u32).saturating_mul(4));
        addr >= start && addr < end
    } else {
        (addr as usize) < app.run.mem_size.saturating_sub(4)
    }
}
