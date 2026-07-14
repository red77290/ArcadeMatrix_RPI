from rgbmatrix import RGBMatrix, RGBMatrixOptions
import logging

class MatrixWrapper:
    def __init__(self, config):
        self.config = config
        self.matrix = None
        self._init_matrix()

    def _init_matrix(self):
        logging.info("Initializing RGB Matrix...")
        options = RGBMatrixOptions()
        
        # Geometry
        options.rows = self.config.matrix_rows
        options.cols = self.config.matrix_cols
        options.chain_length = self.config.matrix_chain
        options.parallel = self.config.matrix_parallel
        
        options.hardware_mapping = self.config.matrix_mapping
        options.brightness = self.config.matrix_brightness
        options.gpio_slowdown = self.config.matrix_slowdown
        options.led_rgb_sequence = self.config.matrix_rgb_sequence
        options.pwm_bits = self.config.matrix_pwm_bits
        options.pwm_lsb_nanoseconds = self.config.matrix_pwm_lsb_nanoseconds
        # Hardware pulsing enabled to eliminate flickering.
        # Requires audio to be disabled in OS.
        options.disable_hardware_pulsing = False
        options.drop_privileges = False

        try:
            self.matrix = RGBMatrix(options=options)
            logging.info(f"Matrix initialized: {options.cols * options.chain_length}x{options.rows} (Mapping: {options.hardware_mapping})")
        except Exception as e:
            logging.error(f"Failed to initialize rgbmatrix: {e}")
            logging.error("Make sure you are running as root (sudo) and have compiled hzeller's library.")
            self.matrix = None

    def clear(self):
        if self.matrix:
            self.matrix.Clear()

    def get_canvas(self):
        if self.matrix:
            return self.matrix.CreateFrameCanvas()
        return None

    def swap_canvas(self, canvas):
        if self.matrix and canvas:
            return self.matrix.SwapOnVSync(canvas)
        return canvas
