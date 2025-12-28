//! Linux-specific GTK thread management.

use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use crate::error::WebViewError;

type GtkTask = Box<dyn FnOnce() + Send + 'static>;

struct GtkRunner {
    sender: mpsc::Sender<GtkTask>,
    init_error: Option<String>,
}

static GTK_RUNNER: OnceLock<GtkRunner> = OnceLock::new();

fn gtk_runner() -> Result<&'static GtkRunner, WebViewError> {
    let runner = GTK_RUNNER.get_or_init(|| {
        let (task_tx, task_rx) = mpsc::channel::<GtkTask>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        std::thread::spawn(move || {
            let init_result = gtk::init().map_err(|err| err.to_string());
            let _ = init_tx.send(init_result.clone());

            if init_result.is_err() {
                return;
            }

            loop {
                while let Ok(task) = task_rx.try_recv() {
                    task();
                }
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        });

        let init_result = init_rx
            .recv()
            .unwrap_or_else(|_| Err("gtk init thread failed".to_string()));

        GtkRunner {
            sender: task_tx,
            init_error: init_result.err(),
        }
    });

    if let Some(err) = runner.init_error.as_ref() {
        return Err(WebViewError::GtkInit(err.clone()));
    }

    Ok(runner)
}

/// Runs a closure on the dedicated GTK thread.
pub fn run_on_gtk_thread<F, R>(f: F) -> Result<R, WebViewError>
where
    F: FnOnce() -> Result<R, WebViewError> + Send + 'static,
    R: Send + 'static,
{
    let runner = gtk_runner()?;
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    runner
        .sender
        .send(Box::new(move || {
            let result = f();
            let _ = result_tx.send(result);
        }))
        .map_err(|_| WebViewError::Internal("gtk runner stopped".to_string()))?;

    result_rx
        .recv()
        .map_err(|_| WebViewError::Internal("gtk runner stopped".to_string()))?
}

/// Ensures GTK is initialized on the current thread.
pub fn ensure_gtk_initialized() -> Result<(), WebViewError> {
    gtk::init().map_err(|err| WebViewError::GtkInit(err.to_string()))
}
