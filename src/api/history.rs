#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Timeframe {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl Timeframe {
    pub fn from_str_opt(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hourly" | "1h" | "60m" => Timeframe::Hourly,
            "weekly" | "7d" | "1w" => Timeframe::Weekly,
            "monthly" | "30d" | "1m" | "1mo" => Timeframe::Monthly,
            _ => Timeframe::Daily,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::Hourly => "hourly",
            Timeframe::Daily => "daily",
            Timeframe::Weekly => "weekly",
            Timeframe::Monthly => "monthly",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Timeframe::Hourly => "1H",
            Timeframe::Daily => "1D",
            Timeframe::Weekly => "7D",
            Timeframe::Monthly => "1M",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PriceHistory {
    pub points: Vec<f64>,
    pub min: f64,
    pub max: f64,
}

impl PriceHistory {
    pub fn from_raw(raw: &[f64]) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }

        let mut min = f64::MAX;
        let mut max = f64::MIN;
        let mut valid_points = Vec::with_capacity(raw.len());

        for &p in raw {
            if p.is_finite() && p > 0.0 {
                if p < min {
                    min = p;
                }
                if p > max {
                    max = p;
                }
                valid_points.push(p);
            }
        }

        if valid_points.is_empty() {
            return None;
        }

        if (max - min).abs() < f64::EPSILON {
            max = min + 1.0;
        }

        Some(Self {
            points: valid_points,
            min,
            max,
        })
    }

    pub fn downsample(&self, target_len: usize) -> Self {
        if self.points.len() <= target_len || target_len == 0 {
            return self.clone();
        }

        let mut res = Vec::with_capacity(target_len);
        let chunk_size = self.points.len() as f64 / target_len as f64;

        for i in 0..target_len {
            let start = (i as f64 * chunk_size).floor() as usize;
            let end = (((i + 1) as f64 * chunk_size).ceil() as usize).min(self.points.len());

            if start >= end {
                if let Some(&p) = self.points.get(start.min(self.points.len() - 1)) {
                    res.push(p);
                }
            } else {
                let slice = &self.points[start..end];
                let sum: f64 = slice.iter().sum();
                res.push(sum / slice.len() as f64);
            }
        }

        Self {
            points: res,
            min: self.min,
            max: self.max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeframe_parsing() {
        assert_eq!(Timeframe::from_str_opt("daily"), Timeframe::Daily);
        assert_eq!(Timeframe::from_str_opt("weekly"), Timeframe::Weekly);
        assert_eq!(Timeframe::from_str_opt("7d"), Timeframe::Weekly);
        assert_eq!(Timeframe::from_str_opt("monthly"), Timeframe::Monthly);
        assert_eq!(Timeframe::from_str_opt("unknown"), Timeframe::Daily);
    }

    #[test]
    fn test_price_history_downsample() {
        let raw: Vec<f64> = (1..=100).map(|v| v as f64).collect();
        let history = PriceHistory::from_raw(&raw).unwrap();
        assert_eq!(history.min, 1.0);
        assert_eq!(history.max, 100.0);

        let sampled = history.downsample(10);
        assert_eq!(sampled.points.len(), 10);
        assert_eq!(sampled.min, 1.0);
        assert_eq!(sampled.max, 100.0);
        assert!(sampled.points[0] < sampled.points[9]);
    }
}
