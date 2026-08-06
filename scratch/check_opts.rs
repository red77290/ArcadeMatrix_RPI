use rpi_led_matrix::LedMatrixOptions;
fn main() {
    let mut opts = LedMatrixOptions::new();
    opts.set_disable_hardware_pulsing(true);
}
