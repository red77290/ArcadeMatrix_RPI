use chrono::{Local, NaiveTime};

pub fn is_night_time(enabled: bool, turn_off_at: &str, wake_up_at: &str) -> bool {
    if !enabled {
        return false;
    }

    let now = Local::now().time();
    let turn_off = match NaiveTime::parse_from_str(turn_off_at, "%H:%M") {
        Ok(t) => t,
        Err(_) => return false,
    };
    let wake_up = match NaiveTime::parse_from_str(wake_up_at, "%H:%M") {
        Ok(t) => t,
        Err(_) => return false,
    };

    if turn_off <= wake_up {
        // Same-day range (e.g. 09:00 to 17:00)
        now >= turn_off && now < wake_up
    } else {
        // Overnight range (e.g. 23:00 to 07:00)
        now >= turn_off || now < wake_up
    }
}

pub struct RotationState {
    pub current_index: usize,
    pub mode_start_time: std::time::Instant,
}

impl RotationState {
    pub fn new() -> Self {
        Self {
            current_index: 0,
            mode_start_time: std::time::Instant::now(),
        }
    }

    pub fn next_mode<'a>(
        &mut self,
        rotation_list: &'a [crate::core::config::RotationEntry],
    ) -> Option<&'a crate::core::config::RotationEntry> {
        if rotation_list.is_empty() {
            return None;
        }
        self.current_index = self.current_index.wrapping_add(1);
        self.mode_start_time = std::time::Instant::now();
        Some(&rotation_list[self.current_index % rotation_list.len()])
    }
}
