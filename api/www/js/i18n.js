export const translations = {
  en: {
    nav_dashboard: "Dashboard",
    nav_display: "Display",
    nav_media: "Media",
    nav_message: "Message",
    fighter_title: "Fighter Overlay",
    fighter_desc: "Animated fighter sprites composited on top of idle rotation screens. Enable it per screen from the Rotation panel.",
    fighter_enabled: "Enabled",
    fighter_interval: "Interval between fights (seconds)",
    fighter_save: "Save Fighter Settings",
    nav_system: "System",
    ota_drop: "Drop firmware binary here or click to browse",
    ota_confirm: "This will replace the running firmware and restart the device. Continue?",
    
    // Tooltips
    tt_hw_chain: "Number of LED panels connected sequentially in a single line.",
    tt_hw_parallel: "Number of parallel chains (requires specific wiring/HAT).",
    tt_hw_mapping: "Hardware mapping: 'regular' for standard wiring, 'regular-pi1' for Joy-IT HAT, 'adafruit-hat-pwm' if you soldered the PWM mod.",
    tt_hw_rgb: "Change this if your colors appear swapped (e.g., Red looks Blue).",
    tt_hw_slowdown: "GPIO Slowdown. Crucial for modern Raspberry Pis to prevent flickering (Pi 3 = 1-2, Pi 4 = 2-3, Pi 5 = 4).",
    tt_hw_pwm_bits: "Lowering this increases refresh rate but reduces color depth. 11 is default.",
    tt_hw_pwm_lsb: "Adjust only if you experience ghosting or brightness artifacts. Default 130.",
    tt_hw_disable_pulsing: "DANGER: Checking this disables DMA and uses CPU spin-loops (100% CPU usage), which WILL freeze single-core Pis. Keep UNCHECKED for DMA (requires disabling OS audio).",
    tt_hw_limit_refresh: "Refresh rate limit. Set to 0 to uncap (forces 120Hz internally to prevent CPU lockups).",
  },
  fr: {
    nav_dashboard: "Tableau de bord",
    nav_display: "Affichage",
    nav_media: "Médias",
    nav_message: "Message",
    fighter_title: "Overlay Combattant",
    fighter_desc: "Sprites de combattants animés superposés sur les écrans de rotation en veille. À activer par écran depuis le panneau Rotation.",
    fighter_enabled: "Activé",
    fighter_interval: "Intervalle entre les combats (secondes)",
    fighter_save: "Enregistrer les paramètres Combattant",
    nav_system: "Système",
    ota_drop: "Glissez le binaire ici ou cliquez pour parcourir",
    ota_confirm: "Cela va remplacer le firmware en cours et redémarrer l'appareil. Continuer ?",
    
    // Tooltips
    tt_hw_chain: "Nombre de dalles LED chaînées à la suite sur une seule ligne.",
    tt_hw_parallel: "Nombre de chaînes parallèles (nécessite un câblage/HAT spécifique).",
    tt_hw_mapping: "Mapping matériel : 'regular' (standard), 'regular-pi1' (HAT Joy-IT), 'adafruit-hat-pwm' (si modif PWM soudée).",
    tt_hw_rgb: "Modifiez ceci si vos couleurs sont inversées (ex: le rouge apparait bleu).",
    tt_hw_slowdown: "Ralentissement GPIO. Indispensable pour éviter le clignotement sur les Pi récents (Pi 3 = 1-2, Pi 4 = 2-3).",
    tt_hw_pwm_bits: "Baisser cette valeur augmente la fluidité mais réduit les nuances de couleurs. 11 par défaut.",
    tt_hw_pwm_lsb: "Ajustez uniquement en cas d'effets fantômes (ghosting). 130 par défaut.",
    tt_hw_disable_pulsing: "DANGER : Cocher ceci désactive le DMA et sature le CPU à 100%, ce qui plantera les Pi monocœurs ! Laissez DÉCOCHÉ pour utiliser le DMA (nécessite de couper l'audio de l'OS).",
    tt_hw_limit_refresh: "Limite de rafraîchissement. 0 pour illimité (forcé à 120Hz en interne pour éviter un freeze CPU).",
  },
  es: {
    nav_dashboard: "Panel",
    nav_display: "Pantalla",
    nav_media: "Medios",
    nav_message: "Mensaje",
    fighter_title: "Superposición Luchador",
    fighter_desc: "Sprites de luchadores animados superpuestos sobre las pantallas de rotación inactivas. Actívalo por pantalla desde el panel de Rotación.",
    fighter_enabled: "Activado",
    fighter_interval: "Intervalo entre combates (segundos)",
    fighter_save: "Guardar ajustes de Luchador",
    nav_system: "Sistema",
    ota_drop: "Arrastra el binario aquí o haz clic para buscar",
    ota_confirm: "Esto reemplazará el firmware y reiniciará el dispositivo. ¿Continuar?",
    
    // Tooltips
    tt_hw_chain: "Número de paneles LED encadenados en una sola línea.",
    tt_hw_parallel: "Número de cadenas paralelas (requiere cableado/HAT específico).",
    tt_hw_mapping: "Mapeo de hardware: 'regular' (estándar), 'regular-pi1' (Joy-IT HAT), 'adafruit-hat-pwm' (con mod PWM).",
    tt_hw_rgb: "Cambia esto si tus colores aparecen intercambiados (ej: rojo se ve azul).",
    tt_hw_slowdown: "Ralentización GPIO. Crucial para evitar parpadeos en Pi recientes (Pi 3 = 1-2, Pi 4 = 2-3).",
    tt_hw_pwm_bits: "Bajar este valor aumenta la tasa de refresco pero reduce la profundidad de color. 11 por defecto.",
    tt_hw_pwm_lsb: "Ajusta esto solo si experimentas efecto fantasma (ghosting). 130 por defecto.",
    tt_hw_disable_pulsing: "PELIGRO: Marcar esto desactiva DMA y satura el CPU al 100%, lo que congelará las Pi de un solo núcleo. Dejar DESMARCADO (requiere desactivar el audio del SO).",
    tt_hw_limit_refresh: "Límite de refresco. 0 para ilimitado (forzado a 120Hz internamente para evitar el bloqueo del CPU).",
  }
};

export function setLanguage(lang) {
  const dict = translations[lang] || translations.en;
  
  // Text content translation
  document.querySelectorAll('[data-i18n]').forEach(el => {
    const key = el.getAttribute('data-i18n');
    if (dict[key]) {
      el.textContent = dict[key];
    }
  });

  // Tooltip translation
  document.querySelectorAll('[data-i18n-tooltip]').forEach(el => {
    const key = el.getAttribute('data-i18n-tooltip');
    if (dict[key]) {
      el.setAttribute('data-tooltip', dict[key]);
      el.setAttribute('tabindex', '0'); // For mobile focus
    }
  });
}
