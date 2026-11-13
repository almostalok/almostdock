#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub epsilon: f32,
}

impl Spring {
    pub fn new(initial: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            value: initial,
            velocity: 0.0,
            target: initial,
            stiffness,
            damping,
            epsilon: 0.001,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn reset(&mut self, value: f32) {
        self.value = value;
        self.velocity = 0.0;
        self.target = value;
    }

    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < self.epsilon && self.velocity.abs() < self.epsilon
    }

    /// Advances the spring simulation by `dt` seconds using a semi-implicit Euler integration
    /// with sub-stepping for extreme stability at any frame rate.
    pub fn update(&mut self, dt: f32) -> bool {
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
            return false;
        }

        // Sub-step to prevent explosion on large frame drops (e.g. max step 4ms)
        let max_step = 0.004;
        let mut remaining = dt.min(0.1); // clamp frame delta to 100ms max

        while remaining > 0.0 {
            let step = remaining.min(max_step);

            let displacement = self.value - self.target;
            let spring_force = -self.stiffness * displacement;
            let damping_force = -self.damping * self.velocity;
            let acceleration = spring_force + damping_force;

            self.velocity += acceleration * step;
            self.value += self.velocity * step;

            remaining -= step;
        }

        if (self.value - self.target).abs() < self.epsilon && self.velocity.abs() < self.epsilon {
            self.value = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }
}
