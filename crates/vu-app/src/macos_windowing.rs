unsafe extern "C" {
    fn vu_install_window_cycle_shortcuts();
    fn vu_cycle_app_window(reverse: bool);
}

pub fn install_window_cycle_shortcuts() {
    unsafe { vu_install_window_cycle_shortcuts() };
}

pub fn cycle_app_window(reverse: bool) {
    unsafe { vu_cycle_app_window(reverse) };
}
