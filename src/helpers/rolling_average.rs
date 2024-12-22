#[derive(Debug, Clone)]
pub struct RollingAverage<const CAP: usize> {
    data: [f64; CAP],
    sum: f64,
    idx: usize,
    count: usize,
}

impl<const CAP: usize> RollingAverage<CAP> {
    pub fn new () -> Self {
        Self {
            data: [0.0; CAP],
            sum: 0.0,
            idx: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, val: f64) {
        if self.count == CAP {
            let oldest = self.data[self.idx];
            self.sum -= oldest;
        } else {
            self.count += 1;
        }

        self.data[self.idx] = val;
        self.sum += val;

        self.idx = (self.idx + 1) % CAP;
    }

    pub fn average(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }
}