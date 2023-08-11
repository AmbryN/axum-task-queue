use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{mpsc::Sender, Arc, Mutex, RwLock},
    time::Duration,
};
use tracing::instrument::WithSubscriber;

struct AppState {
    current_number_tasks: u8,
    index: usize,
    queue: Queue,
}

impl AppState {
    fn new() -> AppState {
        AppState {
            current_number_tasks: 0,
            index: 0,
            queue: Queue::new(),
        }
    }
}
type SharedAppState = Arc<AppState>;

const MAX_CONCURRENT_TASKS: u8 = 2;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // Initialiazing the channel for sending the tasks to and from the computation thread
    let (tx_task, rx_task) = std::sync::mpsc::channel::<Arc<RwLock<Task>>>();

    // Initialiazing the shared state of the application
    let shared_state: SharedAppState = Arc::new(AppState::new());

    // Channel used to keep track of the current number of tasks being computed
    let (tx_nb, rx_nb) = std::sync::mpsc::channel::<u8>();

    // Spawn the computation thread
    std::thread::spawn(move || {
        {
            println!("Démarrage du thread de calcul");
            loop {
                println!("Début de la loop");
                // When a new task is sent down the channel
                let thread_task = rx_task.recv().unwrap();

                let worker_tx_nb = tx_nb.clone();
                // Spawn a worker thread for the task

                std::thread::spawn(move || {
                    // Acquire the lock on the task
                    let task = thread_task.read().unwrap();

                    println!("On commence à calculer");
                    // Compute it synchronously
                    let result = task.compute();
                    println!("On finit de calculer");
                    drop(task);

                    println!("{:?}", result);

                    println!("On met à jour la tâche");
                    thread_task.write().unwrap().result = Some(result);

                    // Tell the main thread a task is finished
                    worker_tx_nb.send(1).unwrap();
                });

                println!("Fin de la loop");
            }
        }
    });

    // Spawn the feeder thread
    let feeder_state = Arc::clone(&shared_state);
    std::thread::spawn(move || {
        let mut current_number_tasks = 0;
        let mut index = 0;
        loop {
            let state = feeder_state;
            let tasks = &state.queue.tasks;

            if current_number_tasks < MAX_CONCURRENT_TASKS && index < tasks.len() {
                let task_to_feed: Arc<RwLock<Task>> = Arc::clone(&tasks[index]);
                tx_task.send(task_to_feed).unwrap();
                index += 1;
                current_number_tasks += 1;
            }

            if let Ok(_) = rx_nb.recv_timeout(Duration::from_millis(100)) {
                current_number_tasks -= 1;
            };
        }
    });

    // build our application with a route
    let app = Router::new()
        .route("/tasks", get(tasks))
        .route("/tasks", post(create_task))
        .with_state(Arc::clone(&shared_state));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[debug_handler]
async fn tasks(state: State<SharedAppState>) -> Json<Vec<Task>> {
    let queue = state..queue.tasks.clone();
    Json(
        queue
            .iter()
            .map(|task| task.read().unwrap().clone())
            .collect(),
    )
}

#[debug_handler]
async fn create_task(state: State<SharedAppState>, Json(payload): Json<Task>) -> Json<Task> {
    let task = Task {
        duration: payload.duration,
        result: None,
    };

    let task_to_return = task.clone();
    state.queue.enqueue(task);

    Json(task_to_return)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    duration: u64,
    result: Option<u64>,
}

impl Task {
    fn compute(&self) -> u64 {
        let duration: u64 = self.duration as u64;
        std::thread::sleep(Duration::from_secs(duration));
        duration
    }
}

#[derive(Debug, Default, Clone)]
struct Queue {
    tasks: Vec<Arc<RwLock<Task>>>,
}

impl Queue {
    fn new() -> Queue {
        Queue {
            tasks: vec![
                Arc::new(RwLock::new(Task {
                    duration: 3,
                    result: None,
                })),
                Arc::new(RwLock::new(Task {
                    duration: 5,
                    result: None,
                })),
            ],
        }
    }

    fn enqueue(&mut self, task: Task) -> Arc<RwLock<Task>> {
        let task = Arc::new(RwLock::new(task));
        self.tasks.push(Arc::clone(&task));
        task
    }
}
