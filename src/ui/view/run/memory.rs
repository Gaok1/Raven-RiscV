use super::App;

pub(super) fn imem_address_in_range(app: &App, addr: u32) -> bool {
    app.text_exec_region()
        .is_some_and(|region| region.contains(addr))
}

pub(super) fn exec_address_in_range(app: &App, addr: u32) -> bool {
    app.pc_in_executable_region(addr)
}
