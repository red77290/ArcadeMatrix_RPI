use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisplayPriority {
    Rotation = 10,
    Gif = 20,
    Marquee = 30,
    Visualizer = 40,
    Mqtt = 100,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestLifecycle {
    OneShot,
    Timed,
    UntilCancelled,
    Persistent,
}

#[derive(Debug, Clone)]
pub struct DisplayRequest {
    pub source: String,
    pub priority: DisplayPriority,
    pub lifecycle: RequestLifecycle,
    pub preemptive: bool,
    pub instance_id: String,
    pub timeout: Option<Duration>,
    pub created_at: Instant,
}

impl DisplayRequest {
    pub fn new(source: &str, priority: DisplayPriority, lifecycle: RequestLifecycle) -> Self {
        Self {
            source: source.to_string(),
            priority,
            lifecycle,
            preemptive: true,
            instance_id: String::new(),
            timeout: None,
            created_at: Instant::now(),
        }
    }
}

pub struct DisplayArbiter {
    requests: Vec<DisplayRequest>,
}

impl DisplayArbiter {
    pub fn new() -> Self {
        let mut arbiter = Self {
            requests: Vec::new(),
        };

        let mut rot_req = DisplayRequest::new(
            "ROTATION",
            DisplayPriority::Rotation,
            RequestLifecycle::Persistent,
        );
        rot_req.preemptive = false;
        rot_req.instance_id = "rotation_manager".to_string();
        arbiter.requests.push(rot_req);

        arbiter
    }

    pub fn submit_request(&mut self, mut request: DisplayRequest) {
        request.created_at = Instant::now();
        if let Some(pos) = self
            .requests
            .iter()
            .position(|r| r.source == request.source)
        {
            self.requests[pos] = request;
        } else {
            self.requests.push(request);
        }
    }

    pub fn cancel_request(&mut self, source: &str) {
        self.requests.retain(|r| r.source != source);
    }

    pub fn clear_expired(&mut self) {
        let now = Instant::now();
        self.requests.retain(|r| match r.lifecycle {
            RequestLifecycle::OneShot => false,
            RequestLifecycle::Timed => {
                if let Some(timeout) = r.timeout {
                    now.duration_since(r.created_at) < timeout
                } else {
                    true
                }
            }
            _ => true,
        });
    }

    pub fn evaluate(&mut self) -> Option<DisplayRequest> {
        self.clear_expired();
        self.requests.iter().max_by_key(|r| r.priority).cloned()
    }
}
