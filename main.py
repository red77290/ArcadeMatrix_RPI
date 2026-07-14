import sys
import traceback

def handle_exception(exc_type, exc_value, exc_traceback):
    with open("crash.log", "w") as f:
        traceback.print_exception(exc_type, exc_value, exc_traceback, file=f)
    sys.__excepthook__(exc_type, exc_value, exc_traceback)
sys.excepthook = handle_exception

import time
import logging
import threading
import os
import subprocess
import socket
from PIL import Image, ImageDraw, ImageFont
from core.config import Config
from core.matrix import MatrixWrapper
from core.rotation import RotationManager
from api.server import run_server, set_app_instance

# Optional: paho-mqtt for Batocera integration
try:
    import paho.mqtt.client as mqtt
    MQTT_AVAILABLE = True
except ImportError:
    MQTT_AVAILABLE = False

class ArcadeMatrixApp:
    def __init__(self):
        logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
        self.config = Config()
        self.mw = MatrixWrapper(self.config)
        self.rotation_manager = RotationManager(self.mw, self.config)
        self.mqtt_client = None

    def _on_mqtt_message(self, client, userdata, msg):
        payload = msg.payload.decode('utf-8')
        logging.info(f"MQTT Message received on {msg.topic}: {payload}")
        # Here we could interrupt the rotation to display game info
        # e.g., self.rotation_manager.interrupt_with_game(payload)

    def _setup_mqtt(self):
        if not MQTT_AVAILABLE or not self.config.mqtt_enabled:
            return
            
        logging.info(f"Connecting to MQTT broker at {self.config.mqtt_broker}:{self.config.mqtt_port}")
        self.mqtt_client = mqtt.Client(client_id=self.config.mqtt_device)
        
        if self.config.mqtt_user and self.config.mqtt_pass:
            self.mqtt_client.username_pw_set(self.config.mqtt_user, self.config.mqtt_pass)
            
        self.mqtt_client.on_message = self._on_mqtt_message
        
        try:
            self.mqtt_client.connect(self.config.mqtt_broker, self.config.mqtt_port, 60)
            if self.config.mqtt_topic_bato:
                self.mqtt_client.subscribe(self.config.mqtt_topic_bato)
            if self.config.mqtt_topic_recal:
                self.mqtt_client.subscribe(self.config.mqtt_topic_recal)
            self.mqtt_client.loop_start()
            logging.info("MQTT connected and subscribed.")
        except Exception as e:
            logging.error(f"MQTT connection failed: {e}")

    def _setup_wifi(self):
        if self.config.wifi_ssid and not self.config.wifi_configured:
            logging.info(f"Attempting to configure Wi-Fi for SSID: {self.config.wifi_ssid}")
            try:
                # Use nmcli to connect to Wi-Fi
                cmd = f'sudo nmcli dev wifi connect "{self.config.wifi_ssid}" password "{self.config.wifi_pass}"'
                result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
                if result.returncode == 0:
                    logging.info("Wi-Fi successfully connected via nmcli!")
                    self.config.wifi_configured = True
                    self.config.save()
                else:
                    logging.error(f"Failed to connect to Wi-Fi: {result.stderr}")
            except Exception as e:
                logging.error(f"Exception during Wi-Fi setup: {e}")

    def _get_ip(self):
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            # doesn't even have to be reachable
            s.connect(('10.255.255.255', 1))
            IP = s.getsockname()[0]
        except Exception:
            IP = '127.0.0.1'
        finally:
            s.close()
        return IP

    def _show_ip(self):
        ip_addr = self._get_ip()
        logging.info(f"Local IP Address: {ip_addr}")
        try:
            img = Image.new('RGB', (self.config.matrix_width, self.config.matrix_height), "black")
            draw = ImageDraw.Draw(img)
            # Try to load a small font, fallback to default
            try:
                font = ImageFont.truetype("fonts/PressStart2P.ttf", 6)
            except:
                font = ImageFont.load_default()
            
            draw.text((2, 2), "IP Address:", font=font, fill=(0, 255, 0))
            draw.text((2, 14), ip_addr, font=font, fill=(255, 255, 255))
            
            self.mw.set_image(img)
            time.sleep(5)
        except Exception as e:
            logging.error(f"Failed to display IP: {e}")

    def run(self):
        # 0. Setup Wi-Fi if needed
        self._setup_wifi()

        # 0.5 Show IP on Matrix
        self._show_ip()

        # 1. Start Web Server in a separate thread
        set_app_instance(self)
        web_thread = threading.Thread(target=run_server, args=(8080,), daemon=True)
        web_thread.start()

        # 2. Setup MQTT
        self._setup_mqtt()

        # 3. Start Main Rotation Loop (blocking)
        try:
            self.rotation_manager.start_loop()
        except KeyboardInterrupt:
            logging.info("Exiting ArcadeMatrix RPi...")
        finally:
            self.mw.clear()
            if self.mqtt_client:
                self.mqtt_client.loop_stop()

if __name__ == "__main__":
    app = ArcadeMatrixApp()
    app.run()
