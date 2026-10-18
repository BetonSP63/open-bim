//! Long operations on a worker thread with progress and cancellation; the UI
//! stays responsive and the model is changed only when a job completes.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

#[derive(Clone, Default)]
pub struct Progress {
    cancelled: Arc<AtomicBool>,
    done: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
}

impl Progress {
    pub fn set(&self, done: usize, total: usize) {
        self.done.store(done, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| self.done.load(Ordering::Relaxed) as f32 / total as f32)
    }
    pub fn counts(&self) -> (usize, usize) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

pub enum Poll<T> {
    Running,
    Done(Result<T, String>),
    Cancelled,
}
// `Poll<()>` reports state only; the value is taken with `Job::wait`.

pub struct Job<T> {
    pub label: String,
    pub progress: Progress,
    pub started: Instant,
    receiver: mpsc::Receiver<Result<T, String>>,
    ready: std::cell::RefCell<Option<Result<T, String>>>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(
        label: impl Into<String>,
        work: impl FnOnce(&Progress) -> Result<T, String> + Send + 'static,
    ) -> Self {
        let progress = Progress::default();
        let (sender, receiver) = mpsc::channel();
        let worker = progress.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&worker)))
                .unwrap_or_else(|_| Err("Внутренняя ошибка фоновой операции".into()));
            let _ = sender.send(result);
        });
        Self {
            label: label.into(),
            progress,
            started: Instant::now(),
            receiver,
            ready: std::cell::RefCell::new(None),
        }
    }

    /// Request cancellation. Cancellable work stops at its next checkpoint;
    /// otherwise its result is discarded when it arrives.
    pub fn cancel(&self) {
        self.progress.cancelled.store(true, Ordering::Relaxed);
    }

    /// Non-blocking state; a finished result is kept for `wait`.
    pub fn poll(&self) -> Poll<()> {
        if self.progress.cancelled() {
            return Poll::Cancelled;
        }
        if self.ready.borrow().is_some() {
            return Poll::Done(Ok(()));
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                *self.ready.borrow_mut() = Some(result);
                Poll::Done(Ok(()))
            }
            Err(mpsc::TryRecvError::Empty) => Poll::Running,
            Err(mpsc::TryRecvError::Disconnected) => {
                *self.ready.borrow_mut() = Some(Err("Фоновая операция прервана".into()));
                Poll::Done(Ok(()))
            }
        }
    }

    /// Result of a finished job, or a blocking wait for it.
    pub fn wait(self) -> Result<T, String> {
        if let Some(result) = self.ready.into_inner() {
            return result;
        }
        self.receiver
            .recv()
            .map_err(|_| "Фоновая операция прервана".to_string())?
    }
}
